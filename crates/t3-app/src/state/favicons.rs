//! Project favicons (`ProjectFavicon`, spec 2.8): `assets.createUrl` with a `project-favicon`
//! resource, then an unauthenticated GET of the capability URL. One fetch per
//! (environment, workspace root) per session; the bytes are kept, so a remount never flashes
//! the fallback folder again.
//!
//! Views call [`FaviconStore::get`] while rendering (it starts the fetch on first use) and
//! `cx.observe` the store.

use std::{collections::HashMap, sync::Arc};

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Image, ImageFormat, Task};
use t3_protocol::{
    EnvironmentId,
    methods::AssetsCreateUrl,
    projects::{AssetCreateUrlInput, AssetResource},
};

use super::AppState;

/// Where a project's favicon stands.
#[derive(Clone)]
pub enum Favicon {
    /// Fetching, or waiting for a connection. Show the folder fallback.
    Pending,
    Loaded(Arc<Image>),
    /// The project has none (404) or it could not be decoded. Show the folder fallback.
    Missing,
}

type Key = (EnvironmentId, String);

/// Favicons of every project the sidebar has shown.
#[derive(Default)]
pub struct FaviconStore {
    entries: HashMap<Key, Favicon>,
    tasks: HashMap<Key, Task<()>>,
}

struct GlobalFavicons(Entity<FaviconStore>);

impl Global for GlobalFavicons {}

impl FaviconStore {
    /// The global store, created on first use.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(store) = cx.try_global::<GlobalFavicons>() {
            return store.0.clone();
        }
        let store = cx.new(|_| Self::default());
        cx.set_global(GlobalFavicons(store.clone()));
        store
    }

    /// The loaded favicon, if any. Entries are filled by [`FaviconStore::request`].
    pub fn loaded(&self, environment_id: &EnvironmentId, cwd: &str) -> Option<Arc<Image>> {
        match self.entries.get(&(environment_id.clone(), cwd.to_owned())) {
            Some(Favicon::Loaded(image)) => Some(image.clone()),
            _ => None,
        }
    }

    /// Starts fetching every key not seen yet.
    pub fn request(&mut self, keys: impl IntoIterator<Item = Key>, cx: &mut Context<Self>) {
        for key in keys {
            if self.entries.contains_key(&key) {
                continue;
            }
            self.entries.insert(key.clone(), Favicon::Pending);
            if let Some(task) = Self::fetch(key.clone(), cx) {
                self.tasks.insert(key, task);
            }
        }
    }

    fn finish(&mut self, key: Key, favicon: Favicon, cx: &mut Context<Self>) {
        self.tasks.remove(&key);
        self.entries.insert(key, favicon);
        cx.notify();
    }

    /// Waits for a connection, then resolves and downloads the favicon. `None` for detached
    /// environments (fixtures): their projects keep the fallback.
    fn fetch(key: Key, cx: &mut Context<Self>) -> Option<Task<()>> {
        let client = AppState::global(cx)
            .read(cx)
            .environment(&key.0, cx)?
            .read(cx)
            .client()?
            .clone();
        Some(cx.spawn(async move |this, cx| {
            let mut status = client.status();
            while !status.borrow_and_update().is_connected() {
                if status.changed().await.is_err() {
                    return;
                }
            }
            let input = AssetCreateUrlInput {
                resource: AssetResource::ProjectFavicon {
                    cwd: key.1.clone(),
                    path: None,
                },
            };
            let favicon = match client.request::<AssetsCreateUrl>(&input).await {
                Ok(url) => match client.fetch_asset(&url.relative_url).await {
                    Ok((bytes, content_type)) => content_type
                        .as_deref()
                        .and_then(|mime| {
                            ImageFormat::from_mime_type(mime.split(';').next()?.trim())
                        })
                        .map_or(Favicon::Missing, |format| {
                            Favicon::Loaded(Arc::new(Image::from_bytes(format, bytes)))
                        }),
                    Err(_) => Favicon::Missing,
                },
                Err(_) => Favicon::Missing,
            };
            this.update(cx, |this, cx| this.finish(key, favicon, cx))
                .ok();
        }))
    }
}
