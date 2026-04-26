//! Route definitions for the Dioxus 0.7 router.
//!
//! Each variant maps to one component. The `AppShell` is wrapped around
//! all top-level routes via the `#[layout]` annotation — sidebar, topbar,
//! and now-playing bar are present on every screen except `/now-playing`
//! (mobile fullscreen).

use crate::components::downloads::downloads_list::DownloadsList;
use crate::components::layout::app_shell::AppShell;
use crate::components::library::album_detail::AlbumDetail;
use crate::components::library::albums_grid::AlbumsGrid;
use crate::components::library::artist_detail::ArtistDetail;
use crate::components::library::artists_list::ArtistsList;
use crate::components::library::genre_browser::GenreBrowser;
use crate::components::library::library_home::LibraryHome;
use crate::components::library::tracks_table::TracksTable;
use crate::components::player::now_playing_full::NowPlayingFull;
use crate::components::playlists::playlist_detail::PlaylistDetail;
use crate::components::playlists::playlist_editor::PlaylistEditor;
use crate::components::playlists::playlists_list::PlaylistsList;
use crate::components::playlists::smart_playlist_editor::SmartPlaylistEditor;
use crate::components::privacy::audit_log_viewer::AuditLogViewer;
use crate::components::privacy::consent_manager::ConsentManager;
use crate::components::privacy::privacy_dashboard::PrivacyDashboard;
use crate::components::search::search_results::SearchResults;
use crate::components::settings::about_page::AboutPage;
use crate::components::settings::appearance_settings::AppearanceSettings;
use crate::components::settings::audio_settings::AudioSettings;
use crate::components::settings::settings_root::SettingsRoot;
use crate::components::settings::storage_settings::StorageSettings;
use crate::components::sources::source_detail::SourceDetail;
use crate::components::sources::sources_list::SourcesList;
use dioxus::prelude::*;

/// All in-app routes.
#[derive(Clone, Routable, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(AppShell)]
        #[route("/")]
        LibraryHome {},

        #[route("/library/tracks")]
        TracksTable {},

        #[route("/library/artists")]
        ArtistsList {},

        #[route("/library/artists/:id")]
        ArtistDetail { id: String },

        #[route("/library/albums")]
        AlbumsGrid {},

        #[route("/library/albums/:id")]
        AlbumDetail { id: String },

        #[route("/library/genres")]
        GenreBrowser {},

        #[route("/playlists")]
        PlaylistsList {},

        #[route("/playlists/smart/:id")]
        SmartPlaylistEditor { id: String },

        #[route("/playlists/:id/edit")]
        PlaylistEditor { id: String },

        #[route("/playlists/:id")]
        PlaylistDetail { id: String },

        #[route("/sources")]
        SourcesList {},

        #[route("/sources/:id")]
        SourceDetail { id: String },

        #[route("/search?:q")]
        SearchResults { q: String },

        #[route("/downloads")]
        DownloadsList {},

        #[route("/settings")]
        SettingsRoot {},

        #[route("/settings/audio")]
        AudioSettings {},

        #[route("/settings/appearance")]
        AppearanceSettings {},

        #[route("/settings/storage")]
        StorageSettings {},

        #[route("/settings/about")]
        AboutPage {},

        #[route("/settings/privacy")]
        PrivacyDashboard {},

        #[route("/settings/privacy/audit")]
        AuditLogViewer {},

        #[route("/settings/privacy/consent")]
        ConsentManager {},

    #[end_layout]

    #[route("/now-playing")]
    NowPlayingFull {},
}
