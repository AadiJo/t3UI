//! The per-thread right panel model (spec 1.2, `rightPanelStore.ts`): which surfaces (tabs) a
//! thread has, which one is active, and whether the panel is open. Pure data; [`super::store`]
//! wraps it in an entity.

/// A tab of the right panel. Terminal and preview are typed slots filled by their owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Surface {
    Diff,
    /// The workspace file browser.
    Files,
    /// One file open for reading. `reveal_request` bumps on every `open_file` so the preview
    /// scrolls to `reveal_line` again even when the path is already open.
    File {
        path: String,
        reveal_line: Option<u32>,
        reveal_request: u64,
    },
    Plan,
    /// Terminal tab (`terminal:{first id}`); owned by the terminal agent.
    Terminal {
        terminal_ids: Vec<String>,
        active_terminal_id: String,
        split_vertical: bool,
    },
    /// Browser preview (`browser:{tab}`, or `browser:new` before `preview.open` returns).
    Preview {
        tab_id: Option<String>,
    },
}

/// Stable identity of a surface (the web `id` string).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceId {
    Diff,
    Files,
    File(String),
    Plan,
    Terminal(String),
    Preview(Option<String>),
}

/// Kinds that `open` / `toggle` address (the singleton surfaces plus preview).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    Diff,
    Files,
    File,
    Plan,
    Terminal,
    Preview,
}

impl Surface {
    pub fn id(&self) -> SurfaceId {
        match self {
            Self::Diff => SurfaceId::Diff,
            Self::Files => SurfaceId::Files,
            Self::File { path, .. } => SurfaceId::File(path.clone()),
            Self::Plan => SurfaceId::Plan,
            Self::Terminal { terminal_ids, .. } => {
                SurfaceId::Terminal(terminal_ids.first().cloned().unwrap_or_default())
            }
            Self::Preview { tab_id } => SurfaceId::Preview(tab_id.clone()),
        }
    }

    pub fn kind(&self) -> SurfaceKind {
        match self {
            Self::Diff => SurfaceKind::Diff,
            Self::Files => SurfaceKind::Files,
            Self::File { .. } => SurfaceKind::File,
            Self::Plan => SurfaceKind::Plan,
            Self::Terminal { .. } => SurfaceKind::Terminal,
            Self::Preview { .. } => SurfaceKind::Preview,
        }
    }
}

/// One thread's panel (`ThreadRightPanelState`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadPanel {
    pub is_open: bool,
    pub active: Option<SurfaceId>,
    pub surfaces: Vec<Surface>,
}

impl ThreadPanel {
    /// The active surface while open (`selectActiveRightPanelSurface`).
    pub fn active_surface(&self) -> Option<&Surface> {
        if !self.is_open {
            return None;
        }
        let active = self.active.as_ref()?;
        self.surfaces.iter().find(|surface| &surface.id() == active)
    }

    /// Nothing to remember: closed, no tabs (the store drops such entries).
    pub fn is_empty(&self) -> bool {
        !self.is_open && self.active.is_none() && self.surfaces.is_empty()
    }

    /// Appends `surface` if its id is new (or replaces it in place), opens, and activates it.
    pub fn upsert(&mut self, surface: Surface) {
        let id = surface.id();
        match self
            .surfaces
            .iter_mut()
            .find(|existing| existing.id() == id)
        {
            Some(existing) => *existing = surface,
            None => self.surfaces.push(surface),
        }
        self.active = Some(id);
        self.is_open = true;
    }

    /// `open(kind)`: singletons upsert; preview reuses the first preview tab or adds
    /// `browser:new`.
    pub fn open(&mut self, kind: SurfaceKind) {
        match kind {
            SurfaceKind::Diff => self.upsert_existing_or(Surface::Diff),
            SurfaceKind::Files => self.upsert_existing_or(Surface::Files),
            SurfaceKind::Plan => self.upsert_existing_or(Surface::Plan),
            SurfaceKind::Preview => {
                match self
                    .surfaces
                    .iter()
                    .find(|surface| surface.kind() == SurfaceKind::Preview)
                {
                    Some(existing) => {
                        self.active = Some(existing.id());
                        self.is_open = true;
                    }
                    None => self.upsert(Surface::Preview { tab_id: None }),
                }
            }
            // Files and terminals need a path or id; see `open_file` / `open_terminal`.
            SurfaceKind::File | SurfaceKind::Terminal => self.is_open = true,
        }
    }

    fn upsert_existing_or(&mut self, surface: Surface) {
        let id = surface.id();
        if self.surfaces.iter().any(|existing| existing.id() == id) {
            self.active = Some(id);
            self.is_open = true;
        } else {
            self.upsert(surface);
        }
    }

    /// `openFile(path, line)`: drops the standalone file browser, reuses or appends
    /// `file:{path}`, bumps its reveal request. Lines below 1 are ignored.
    pub fn open_file(&mut self, path: &str, line: Option<u32>) {
        self.surfaces
            .retain(|surface| surface.kind() != SurfaceKind::Files);
        let reveal_line = line.filter(|line| *line >= 1);
        let id = SurfaceId::File(path.to_owned());
        match self.surfaces.iter_mut().find(|surface| surface.id() == id) {
            Some(Surface::File {
                reveal_line: existing_line,
                reveal_request,
                ..
            }) => {
                *existing_line = reveal_line;
                *reveal_request += 1;
            }
            _ => self.surfaces.push(Surface::File {
                path: path.to_owned(),
                reveal_line,
                reveal_request: 1,
            }),
        }
        self.active = Some(id);
        self.is_open = true;
    }

    /// `openTerminal(id)`.
    pub fn open_terminal(&mut self, terminal_id: &str) {
        self.upsert(Surface::Terminal {
            terminal_ids: vec![terminal_id.to_owned()],
            active_terminal_id: terminal_id.to_owned(),
            split_vertical: false,
        });
    }

    /// `openBrowser(tabId)`: replaces the `browser:new` placeholder.
    pub fn open_browser(&mut self, tab_id: &str) {
        self.surfaces
            .retain(|surface| surface.id() != SurfaceId::Preview(None));
        self.upsert(Surface::Preview {
            tab_id: Some(tab_id.to_owned()),
        });
    }

    pub fn activate(&mut self, id: &SurfaceId) {
        if self.surfaces.iter().any(|surface| &surface.id() == id) {
            self.active = Some(id.clone());
            self.is_open = true;
        }
    }

    /// `closeSurface(id)`: the next active tab is the one now at the closed index (or the last);
    /// the panel stays open only while tabs remain.
    pub fn close_surface(&mut self, id: &SurfaceId) {
        let Some(index) = self.surfaces.iter().position(|surface| &surface.id() == id) else {
            return;
        };
        self.surfaces.remove(index);
        if self.active.as_ref() == Some(id) {
            self.active = self
                .surfaces
                .get(index.min(self.surfaces.len().saturating_sub(1)))
                .map(Surface::id);
        }
        self.is_open = self.is_open && !self.surfaces.is_empty();
    }

    /// `closeOtherSurfaces(id)`.
    pub fn close_others(&mut self, id: &SurfaceId) {
        if self.surfaces.iter().any(|surface| &surface.id() == id) {
            self.surfaces.retain(|surface| &surface.id() == id);
            self.active = Some(id.clone());
            self.is_open = true;
        }
    }

    /// `closeSurfacesToRight(id)`.
    pub fn close_to_right(&mut self, id: &SurfaceId) {
        let Some(index) = self.surfaces.iter().position(|surface| &surface.id() == id) else {
            return;
        };
        self.surfaces.truncate(index + 1);
        let active_kept = self
            .active
            .as_ref()
            .is_some_and(|active| self.surfaces.iter().any(|surface| &surface.id() == active));
        if !active_kept {
            self.active = Some(id.clone());
        }
    }

    /// `closeAllSurfaces`.
    pub fn close_all(&mut self) {
        self.surfaces.clear();
        self.active = None;
        self.is_open = false;
    }

    /// `toggle(kind)`: closes the panel when it shows that kind, else opens it.
    pub fn toggle(&mut self, kind: SurfaceKind) {
        if self.active_surface().map(Surface::kind) == Some(kind) {
            self.is_open = false;
        } else {
            self.open(kind);
        }
    }

    /// `toggleVisibility`: flips `is_open` and keeps the tabs.
    pub fn toggle_visibility(&mut self) {
        self.is_open = !self.is_open;
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes: duplicate singleton tabs; `open_file` leaving the file browser around or
    //! not re-revealing an already open file; zero/negative reveal lines kept; closing the
    //! active tab activating the wrong neighbour or leaving the panel open with no tabs;
    //! "close to the right" losing the active tab; `toggle` closing when another kind is shown;
    //! hidden panels reporting an active surface.
    use super::*;

    fn ids(panel: &ThreadPanel) -> Vec<SurfaceId> {
        panel.surfaces.iter().map(Surface::id).collect()
    }

    #[test]
    fn singletons_are_not_duplicated() {
        let mut panel = ThreadPanel::default();
        panel.open(SurfaceKind::Diff);
        panel.open(SurfaceKind::Plan);
        panel.open(SurfaceKind::Diff);
        assert_eq!(ids(&panel), [SurfaceId::Diff, SurfaceId::Plan]);
        assert_eq!(panel.active, Some(SurfaceId::Diff));
        assert!(panel.is_open);
    }

    #[test]
    fn open_file_replaces_the_browser_and_rereveals() {
        let mut panel = ThreadPanel::default();
        panel.open(SurfaceKind::Files);
        panel.open_file("src/a.ts", Some(3));
        panel.open_file("src/a.ts", Some(0));
        assert_eq!(ids(&panel), [SurfaceId::File("src/a.ts".into())]);
        assert_eq!(
            panel.surfaces[0],
            Surface::File {
                path: "src/a.ts".into(),
                reveal_line: None,
                reveal_request: 2
            }
        );
    }

    #[test]
    fn closing_picks_the_neighbour_and_closes_when_empty() {
        let mut panel = ThreadPanel::default();
        panel.open(SurfaceKind::Diff);
        panel.open(SurfaceKind::Plan);
        panel.open_file("a", None);
        panel.activate(&SurfaceId::Plan);
        panel.close_surface(&SurfaceId::Plan);
        assert_eq!(panel.active, Some(SurfaceId::File("a".into())));
        panel.close_surface(&SurfaceId::File("a".into()));
        assert_eq!(panel.active, Some(SurfaceId::Diff));
        panel.close_surface(&SurfaceId::Diff);
        assert!(!panel.is_open);
        assert!(panel.is_empty());
    }

    #[test]
    fn close_to_the_right_keeps_or_moves_active() {
        let mut panel = ThreadPanel::default();
        panel.open(SurfaceKind::Diff);
        panel.open(SurfaceKind::Plan);
        panel.open(SurfaceKind::Files);
        panel.close_to_right(&SurfaceId::Diff);
        assert_eq!(ids(&panel), [SurfaceId::Diff]);
        assert_eq!(panel.active, Some(SurfaceId::Diff));
    }

    #[test]
    fn toggle_and_visibility() {
        let mut panel = ThreadPanel::default();
        panel.toggle(SurfaceKind::Diff);
        assert!(panel.is_open);
        panel.toggle(SurfaceKind::Plan);
        assert_eq!(panel.active, Some(SurfaceId::Plan));
        assert!(panel.is_open);
        panel.toggle(SurfaceKind::Plan);
        assert!(!panel.is_open);
        assert_eq!(panel.active_surface(), None);
        panel.toggle_visibility();
        assert_eq!(panel.active_surface(), Some(&Surface::Plan));
    }

    #[test]
    fn preview_placeholder_is_replaced() {
        let mut panel = ThreadPanel::default();
        panel.open(SurfaceKind::Preview);
        assert_eq!(ids(&panel), [SurfaceId::Preview(None)]);
        panel.open_browser("tab-1");
        assert_eq!(ids(&panel), [SurfaceId::Preview(Some("tab-1".into()))]);
    }
}
