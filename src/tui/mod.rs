use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use image::DynamicImage;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect, Size};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image, Resize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::config::output_path_for;
use crate::desktop::DesktopProfile;
use crate::download::{download_max_res, Progress};
use crate::gac::{discover, dominant_colors, Asset, Discovered, GacClient};
use crate::ranking::{rank, score, Filters};
use crate::wallpaper;

enum Msg {
    Input(Event),
    Discovered(Discovered),
    Thumb { id: String, img: Box<DynamicImage> },
    Enriched(Box<Asset>),
    Download(Progress),
    WallpaperSet(Result<String, String>),
}

struct DownloadUi {
    title: String,
    done: usize,
    total: usize,
    saved: Option<PathBuf>,
    failed: Option<String>,
    then_wallpaper: bool,
}

struct App {
    profile: DesktopProfile,
    client: Arc<GacClient>,
    filters: Filters,
    assets: Vec<Asset>,
    list_state: ListState,
    selected_id: Option<String>,
    thumbs: HashMap<String, DynamicImage>,
    thumb_requested: HashSet<String>,
    protocol: Option<Protocol>,
    protocol_for: Option<String>,
    preview_area: Rect,
    picker: Picker,
    status: String,
    discovering: bool,
    download: Option<DownloadUi>,
    dirty: bool,
    thumb_order: VecDeque<String>,
    tx: UnboundedSender<Msg>,
}

const THUMB_CACHE_CAP: usize = 64;

pub async fn run(
    profile: DesktopProfile,
    client: GacClient,
    seeds: Vec<String>,
    limit: usize,
    filters: Filters,
    want_portrait: Option<bool>,
    art_only: bool,
) -> Result<()> {
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    let mut terminal = ratatui::init();
    let (tx, rx) = unbounded_channel::<Msg>();
    let client = Arc::new(client);

    spawn_input(tx.clone());
    spawn_discovery(client.clone(), seeds, limit, want_portrait, art_only, tx.clone());

    let mut app = App {
        profile,
        client,
        filters,
        assets: Vec::new(),
        list_state: ListState::default(),
        selected_id: None,
        thumbs: HashMap::new(),
        thumb_requested: HashSet::new(),
        protocol: None,
        protocol_for: None,
        preview_area: Rect::new(0, 0, 0, 0),
        picker,
        status: "Discovering artworks from Google Arts & Culture…".to_string(),
        discovering: true,
        download: None,
        dirty: false,
        thumb_order: VecDeque::new(),
        tx,
    };

    let result = app.event_loop(&mut terminal, rx).await;
    ratatui::restore();
    result
}

fn spawn_input(tx: UnboundedSender<Msg>) {
    std::thread::spawn(move || loop {
        match event::read() {
            Ok(ev) => {
                if tx.send(Msg::Input(ev)).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    });
}

fn spawn_discovery(
    client: Arc<GacClient>,
    seeds: Vec<String>,
    limit: usize,
    want_portrait: Option<bool>,
    art_only: bool,
    tx: UnboundedSender<Msg>,
) {
    let (d_tx, mut d_rx) = unbounded_channel::<Discovered>();
    tokio::spawn(async move {
        discover(&client, seeds, limit, want_portrait, art_only, Some(d_tx)).await;
    });
    tokio::spawn(async move {
        while let Some(d) = d_rx.recv().await {
            if tx.send(Msg::Discovered(d)).is_err() {
                break;
            }
        }
    });
}

impl App {
    async fn event_loop(
        &mut self,
        terminal: &mut DefaultTerminal,
        mut rx: UnboundedReceiver<Msg>,
    ) -> Result<()> {
        terminal.draw(|f| self.render(f))?;
        while let Some(msg) = rx.recv().await {
            let mut quit = self.handle(msg);
            while let Ok(msg) = rx.try_recv() {
                if self.handle(msg) {
                    quit = true;
                    break;
                }
            }
            if quit {
                break;
            }
            if self.dirty {
                self.rerank();
                self.dirty = false;
            }
            terminal.draw(|f| self.render(f))?;
        }
        Ok(())
    }

    fn handle(&mut self, msg: Msg) -> bool {
        match msg {
            Msg::Input(Event::Key(k)) if k.kind == KeyEventKind::Press => return self.on_key(k.code),
            Msg::Input(_) => {}
            Msg::Discovered(Discovered::Found { asset, total }) => {
                if self.filters.target_color.is_some() {
                    self.spawn_enrich(*asset.clone());
                }
                self.merge_asset(*asset);
                self.status = format!("Discovering… {total} artworks found");
            }
            Msg::Discovered(Discovered::Done(n)) => {
                self.discovering = false;
                self.status =
                    format!("Found {n} artworks. ↑/↓ browse · Enter download · w set wallpaper · q quit");
            }
            Msg::Discovered(Discovered::Throttled) => {
                self.discovering = false;
                self.status =
                    "⚠ Google Arts & Culture is rate-limiting this IP — no artworks loaded. Wait several minutes, then relaunch.".to_string();
            }
            Msg::Thumb { id, img } => {
                if let Some(a) = self.assets.iter_mut().find(|a| a.id == id) {
                    if a.palette.is_empty() {
                        a.palette = dominant_colors(&img.to_rgb8(), 5);
                    }
                }
                self.thumbs.insert(id.clone(), *img);
                self.thumb_order.push_back(id);
                while self.thumb_order.len() > THUMB_CACHE_CAP {
                    if let Some(old) = self.thumb_order.pop_front() {
                        if self.selected_id.as_deref() != Some(old.as_str()) {
                            self.thumbs.remove(&old);
                        }
                    }
                }
            }
            Msg::Enriched(asset) => {
                self.merge_asset(*asset);
            }
            Msg::Download(p) => self.on_download(p),
            Msg::WallpaperSet(r) => self.on_wallpaper_set(r),
        }
        false
    }

    fn on_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Enter => self.start_download(false),
            KeyCode::Char('w') => self.start_wallpaper(),
            _ => {}
        }
        false
    }

    fn current_index(&self) -> Option<usize> {
        let id = self.selected_id.as_ref()?;
        self.assets.iter().position(|a| &a.id == id)
    }

    fn selected_asset(&self) -> Option<&Asset> {
        self.current_index().map(|i| &self.assets[i])
    }

    fn move_selection(&mut self, delta: i32) {
        if self.assets.is_empty() {
            return;
        }
        let cur = self.current_index().unwrap_or(0) as i32;
        let next = (cur + delta).clamp(0, self.assets.len() as i32 - 1) as usize;
        self.selected_id = Some(self.assets[next].id.clone());
        self.list_state.select(Some(next));
        self.request_thumb(next);
    }

    fn request_thumb(&mut self, index: usize) {
        let Some(asset) = self.assets.get(index) else { return };
        if self.thumbs.contains_key(&asset.id) || self.thumb_requested.contains(&asset.id) {
            return;
        }
        self.thumb_requested.insert(asset.id.clone());
        let client = self.client.clone();
        let tx = self.tx.clone();
        let asset = asset.clone();
        tokio::spawn(async move {
            if let Ok(rgb) = client.fetch_thumbnail(&asset, 640).await {
                let _ = tx.send(Msg::Thumb {
                    id: asset.id.clone(),
                    img: Box::new(DynamicImage::ImageRgb8(rgb)),
                });
            }
        });
    }

    fn spawn_enrich(&self, asset: Asset) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut asset = asset;
            if client.enrich(&mut asset).await.is_ok() {
                let _ = tx.send(Msg::Enriched(Box::new(asset)));
            }
        });
    }

    fn merge_asset(&mut self, incoming: Asset) {
        if let Some(existing) = self.assets.iter_mut().find(|a| a.id == incoming.id) {
            if incoming.max_size.is_some() {
                existing.max_size = incoming.max_size;
            }
            if !incoming.palette.is_empty() {
                existing.palette = incoming.palette;
            }
        } else {
            self.assets.push(incoming);
        }
        self.dirty = true;
    }

    fn rerank(&mut self) {
        let keep = self.selected_id.clone();
        self.assets = rank(std::mem::take(&mut self.assets), &self.profile, &self.filters);
        if self.selected_id.is_none() && !self.assets.is_empty() {
            self.selected_id = Some(self.assets[0].id.clone());
            self.request_thumb(0);
        }
        if let Some(id) = keep {
            if let Some(i) = self.assets.iter().position(|a| a.id == id) {
                self.list_state.select(Some(i));
            }
        } else if !self.assets.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    fn start_download(&mut self, then_wallpaper: bool) {
        let Some(asset) = self.selected_asset().cloned() else { return };
        if self.download.as_ref().map(|d| d.saved.is_none() && d.failed.is_none()).unwrap_or(false) {
            return;
        }
        let out = output_path_for(&asset);
        self.download = Some(DownloadUi {
            title: asset.title.clone(),
            done: 0,
            total: 0,
            saved: None,
            failed: None,
            then_wallpaper,
        });
        self.status = format!("Downloading max-resolution “{}” → {}", asset.title, out.display());

        let http = self.client.http().clone();
        let tx = self.tx.clone();
        let page = asset.page.clone();
        let (p_tx, mut p_rx) = unbounded_channel::<Progress>();
        tokio::spawn(async move {
            let _ = download_max_res(&http, &page, &out, Some(p_tx)).await;
        });
        tokio::spawn(async move {
            while let Some(p) = p_rx.recv().await {
                if tx.send(Msg::Download(p)).is_err() {
                    break;
                }
            }
        });
    }

    fn start_wallpaper(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else { return };
        let out = output_path_for(&asset);
        if out.exists() {
            self.status = format!("Setting wallpaper from {}…", out.display());
            self.spawn_set_wallpaper(out);
        } else {
            self.start_download(true);
        }
    }

    fn spawn_set_wallpaper(&self, path: PathBuf) {
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            let result = wallpaper::set_auto(&path).map_err(|e| e.to_string());
            let _ = tx.send(Msg::WallpaperSet(result));
        });
    }

    fn on_wallpaper_set(&mut self, result: Result<String, String>) {
        self.status = match result {
            Ok(tool) => format!("✓ Wallpaper set via {tool}"),
            Err(e) => format!("Wallpaper failed: {e}"),
        };
    }

    fn on_download(&mut self, p: Progress) {
        if self.download.is_none() {
            return;
        }
        match p {
            Progress::Manifest { tiles, .. } => {
                if let Some(d) = self.download.as_mut() {
                    d.total = tiles;
                }
            }
            Progress::Tile { done, total } => {
                if let Some(d) = self.download.as_mut() {
                    d.done = done;
                    d.total = total;
                }
            }
            Progress::Saved { path, size } => {
                let then_wallpaper = self.download.as_ref().map(|d| d.then_wallpaper).unwrap_or(false);
                if let Some(d) = self.download.as_mut() {
                    d.saved = Some(path.clone());
                }
                self.status = format!("Saved {}×{} to {}", size.0, size.1, path.display());
                if then_wallpaper {
                    self.spawn_set_wallpaper(path);
                }
            }
            Progress::Failed(e) => {
                if let Some(d) = self.download.as_mut() {
                    d.failed = Some(e.clone());
                }
                self.status = format!("Download failed: {e}");
            }
        }
    }

    fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::vertical([
            Constraint::Length(4),
            Constraint::Min(5),
            Constraint::Length(if self.download.is_some() { 3 } else { 1 }),
        ])
        .split(f.area());

        self.render_header(f, chunks[0]);

        let body = Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(chunks[1]);
        self.render_list(f, body[0]);
        self.render_preview(f, body[1]);

        self.render_footer(f, chunks[2]);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let p = &self.profile;
        let res = p
            .resolution
            .map(|(w, h)| format!("{w}×{h}"))
            .unwrap_or_else(|| "unknown".into());
        let scale = p.scale.map(|s| format!(" @{s:.2}x")).unwrap_or_default();
        let font = p.font.clone().unwrap_or_else(|| "?".into());
        let cursor = p.cursor_theme.clone().unwrap_or_else(|| "?".into());
        let bars = if p.bars.is_empty() { "none".into() } else { p.bars.join(", ") };

        let lines = vec![
            Line::from(vec![
                Span::styled("CultureAndArts", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                Span::raw("  —  ideal Google Arts & Culture wallpaper for your desktop"),
            ]),
            Line::from(format!(
                "display {res}{scale} · {} ({}) · font {font} · cursor {cursor} · bars: {bars}",
                p.desktop, p.session_type
            )),
        ];
        let block = Block::default().borders(Borders::ALL);
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn render_list(&mut self, f: &mut Frame, area: Rect) {
        let selected_ratio = self.profile.aspect_ratio();
        let items: Vec<ListItem> = self
            .assets
            .iter()
            .map(|a| {
                let res = a
                    .max_size
                    .map(|(w, h)| format!("{w}×{h}"))
                    .unwrap_or_else(|| "…".into());
                let fit = match (selected_ratio, a.max_size) {
                    (Some(_), Some(_)) => format!("{:.0}%", score(a, &self.profile) * 100.0),
                    _ => "—".into(),
                };
                let creator = if a.creator.is_empty() { "Unknown".to_string() } else { a.creator.clone() };
                let line = Line::from(vec![
                    Span::styled(truncate(&a.title, 34), Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    Span::styled(format!("{} · {}", a.era().label(), res), Style::default().fg(Color::DarkGray)),
                    Span::raw("  "),
                    Span::styled(format!("fit {fit}"), Style::default().fg(Color::Green)),
                    Span::raw(format!("  · {creator}")),
                ]);
                ListItem::new(line)
            })
            .collect();

        let title = format!(" Artworks ({}) ", self.assets.len());
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().bg(Color::Magenta).fg(Color::Black))
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_preview(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Preview ");
        let inner = block.inner(area);
        f.render_widget(block, area);
        self.preview_area = inner;

        if inner.height < 4 {
            return;
        }
        let split = Layout::vertical([Constraint::Min(3), Constraint::Length(5)]).split(inner);
        let img_area = split[0];
        let meta_area = split[1];

        self.ensure_protocol(img_area);
        match (&self.protocol, &self.selected_id, &self.protocol_for) {
            (Some(proto), Some(sel), Some(pf)) if sel == pf => {
                f.render_widget(Image::new(proto), img_area);
            }
            _ => {
                let msg = if self.selected_asset().is_some() {
                    "loading preview…"
                } else {
                    "no selection"
                };
                f.render_widget(Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)), img_area);
            }
        }

        if let Some(asset) = self.selected_asset() {
            let mut lines = vec![
                Line::from(Span::styled(asset.title.clone(), Style::default().add_modifier(Modifier::BOLD))),
                Line::from(format!(
                    "{} · {} · {}",
                    if asset.creator.is_empty() { "Unknown".into() } else { asset.creator.clone() },
                    if asset.date_text.is_empty() { "date unknown".into() } else { asset.date_text.clone() },
                    asset.era().label()
                )),
            ];
            if !asset.palette.is_empty() {
                let swatches: Vec<Span> = asset
                    .palette
                    .iter()
                    .map(|c| Span::styled("  ", Style::default().bg(Color::Rgb(c[0], c[1], c[2]))))
                    .collect();
                lines.push(Line::from(swatches));
            }
            f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), meta_area);
        }
    }

    fn ensure_protocol(&mut self, img_area: Rect) {
        let Some(asset) = self.selected_asset() else { return };
        let id = asset.id.clone();
        if self.protocol_for.as_deref() == Some(id.as_str()) {
            return;
        }
        let Some(img) = self.thumbs.get(&id).cloned() else { return };
        if img_area.width < 2 || img_area.height < 2 {
            return;
        }
        let size = Size::new(img_area.width, img_area.height);
        if let Ok(proto) = self.picker.new_protocol(img, size, Resize::Fit(None)) {
            self.protocol = Some(proto);
            self.protocol_for = Some(id);
        }
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        if let Some(d) = &self.download {
            let ratio = if d.total > 0 { d.done as f64 / d.total as f64 } else { 0.0 };
            let label = if let Some(path) = &d.saved {
                format!("Saved → {}", path.display())
            } else if let Some(err) = &d.failed {
                format!("Failed: {err}")
            } else {
                format!("Downloading “{}” · tile {}/{}", truncate(&d.title, 30), d.done, d.total)
            };
            let color = if d.failed.is_some() { Color::Red } else { Color::Green };
            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL))
                .gauge_style(Style::default().fg(color))
                .ratio(ratio.clamp(0.0, 1.0))
                .label(label);
            f.render_widget(gauge, area);
        } else {
            f.render_widget(
                Paragraph::new(self.status.clone()).style(Style::default().fg(Color::Gray)),
                area,
            );
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
