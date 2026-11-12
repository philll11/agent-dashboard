//! What every surface works with: the history, the data roots, the time zone and the prices.

use std::collections::HashMap;
use std::ffi::OsString;
use std::hash::BuildHasher;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use agent_dashboard_core::history::{self, History};
use agent_dashboard_core::price_refresh;
use agent_dashboard_core::pricing::PriceTable;
use agent_dashboard_core::report::Bucket;
use agent_dashboard_core::roots::{
    self, CONFIG_DIR_ENV, COPILOT_HOME_ENV, DataRoot, VSCODE_APPDATA_ENV, VSCODE_DATA_ENV,
    VSCODE_PORTABLE_ENV,
};
use agent_dashboard_core::usage_endpoint::LoginSource;
use jiff::civil::{Date, Weekday};
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

/// The environment variable that moves the history to another file, for a portable setup or a
/// test.
pub const HISTORY_ENV: &str = "AGENT_DASHBOARD_HISTORY";

/// The environment variable that fixes the time every command sees, as an RFC 3339 timestamp such
/// as `2026-10-10T12:15:00+13:00`, so that tests and the web UI's screenshots do not depend on
/// when they run.
pub const NOW_ENV: &str = "AGENT_DASHBOARD_NOW";

/// The current time, or the time [`NOW_ENV`] fixes.
pub fn now() -> Timestamp {
    static FIXED: LazyLock<Option<Timestamp>> =
        LazyLock::new(|| std::env::var(NOW_ENV).ok()?.parse().ok());
    FIXED.unwrap_or_else(Timestamp::now)
}

/// A day of the week by its English name, such as `monday`, in any case.
#[must_use]
pub fn weekday(name: &str) -> Option<Weekday> {
    WEEKDAYS
        .iter()
        .find(|(day, _)| day.eq_ignore_ascii_case(name))
        .map(|&(_, weekday)| weekday)
}

/// The days of the week by name, Monday first.
pub const WEEKDAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Monday),
    ("tuesday", Weekday::Tuesday),
    ("wednesday", Weekday::Wednesday),
    ("thursday", Weekday::Thursday),
    ("friday", Weekday::Friday),
    ("saturday", Weekday::Saturday),
    ("sunday", Weekday::Sunday),
];

/// The shared setting that holds the IANA time zone days are counted in; without it, this
/// computer's.
pub const TIME_ZONE_SETTING: &str = "time_zone";

/// The shared setting that holds the first day of the week, such as `monday`.
pub const WEEK_START_SETTING: &str = "week_start";

/// The shared setting that holds where the desktop app last ran, which `agent-dashboard app`
/// opens.
pub const DESKTOP_APP_SETTING: &str = "desktop_app";

/// The shared setting that holds whether the desktop app shows the five-hour window in the
/// notification area or the menu bar, as `on` or `off`; without it, it does.
pub const TRAY_SETTING: &str = "tray";

/// The shared setting that holds whether the desktop app keeps running when its window closes,
/// as `on` or `off`; without it, it does on Windows and macOS, and on Linux, where a desktop may
/// show no notification area to come back from, it quits.
pub const KEEP_RUNNING_SETTING: &str = "keep_running";

/// Whether the desktop app shows the five-hour window in the notification area or the menu bar.
#[must_use]
pub fn shows_tray<S: BuildHasher>(settings: &HashMap<String, String, S>) -> bool {
    settings
        .get(TRAY_SETTING)
        .is_none_or(|value| value != "off")
}

/// Whether the desktop app keeps running when its window closes.
#[must_use]
pub fn keeps_running<S: BuildHasher>(settings: &HashMap<String, String, S>) -> bool {
    settings
        .get(KEEP_RUNNING_SETTING)
        .map_or(cfg!(not(target_os = "linux")), |value| value == "on")
}

#[derive(Clone)]
pub struct App {
    pub history_path: Option<PathBuf>,
    pub roots: Vec<DataRoot>,
    /// The value of `CLAUDE_CONFIG_DIR`.
    pub config_dir: Option<OsString>,
    /// The value of `COPILOT_HOME`.
    pub copilot_home: Option<OsString>,
    /// The value of `AGENT_DASHBOARD_VSCODE_DATA`.
    pub vscode_data: Option<OsString>,
    /// The value of `VSCODE_PORTABLE`.
    pub vscode_portable: Option<OsString>,
    /// The value of `VSCODE_APPDATA`.
    pub vscode_appdata: Option<OsString>,
    /// The folder VS Code keeps each product's user data in by default.
    pub app_data: Option<PathBuf>,
    /// The names the agents show for models the price table does not list, by model id.
    pub model_names: Arc<HashMap<String, String>>,
    pub home: Option<PathBuf>,
    /// The time zone days are counted in.
    pub zone: TimeZone,
    /// This computer's time zone.
    pub system_zone: TimeZone,
    /// The first day of the week.
    pub week_start: Weekday,
    pub now: Timestamp,
    /// The price table in use: the one this version ships with, or a newer one from the
    /// repository.
    pub prices: Arc<PriceTable>,
    /// The `agent-dashboard` program that a status line connected from here runs a copy of,
    /// kept next to the history, because an update moves the program itself: this program for
    /// `serve --status-line-copy`, and the desktop app's bundled one. Without it, the status line
    /// runs this program where it is.
    pub status_line_copy: Option<PathBuf>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// The app as this computer and the environment set it up, before the settings the history
    /// holds.
    #[must_use]
    pub fn new() -> Self {
        let history_path = std::env::var_os(HISTORY_ENV)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(history::default_path);
        let home = std::env::home_dir();
        let app_data = roots::app_data(
            home.as_deref(),
            std::env::var_os("APPDATA").as_deref(),
            std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        );
        let zone = TimeZone::system();
        let mut app = Self {
            history_path,
            roots: Vec::new(),
            config_dir: std::env::var_os(CONFIG_DIR_ENV),
            copilot_home: std::env::var_os(COPILOT_HOME_ENV),
            vscode_data: std::env::var_os(VSCODE_DATA_ENV),
            vscode_portable: std::env::var_os(VSCODE_PORTABLE_ENV),
            vscode_appdata: std::env::var_os(VSCODE_APPDATA_ENV),
            app_data,
            model_names: Arc::default(),
            home,
            system_zone: zone.clone(),
            zone,
            week_start: Weekday::Monday,
            now: now(),
            prices: PriceTable::bundled_shared(),
            status_line_copy: None,
        };
        app.roots = roots::data_roots(&app.environment(), &[]);
        app
    }

    /// What the agents' data roots are found from.
    #[must_use]
    pub fn environment(&self) -> roots::Environment<'_> {
        roots::Environment {
            home: self.home.as_deref(),
            app_data: self.app_data.as_deref(),
            claude_config_dir: self.config_dir.as_deref(),
            copilot_home: self.copilot_home.as_deref(),
            vscode_data: self.vscode_data.as_deref(),
            vscode_portable: self.vscode_portable.as_deref(),
            vscode_appdata: self.vscode_appdata.as_deref(),
        }
    }

    /// The app with the settings every surface shares, kept in the history: the data folders the
    /// person added, the time zone, the first day of the week and the price table. Without a
    /// history yet, there is nothing to apply.
    #[must_use]
    pub fn with_shared_settings(self) -> Self {
        match &self.history_path {
            Some(path) if path.is_file() => match History::open(path) {
                Ok(history) => self.with_shared(&history),
                Err(_) => self,
            },
            _ => self,
        }
    }

    /// The app with the settings `history` holds for every surface.
    #[must_use]
    pub fn with_shared(&self, history: &History) -> Self {
        let mut app = self.clone();
        if let Ok(folders) = history.data_folders() {
            app.roots = roots::data_roots(&app.environment(), &folders);
        }
        if let Ok(names) = history.model_names() {
            app.model_names = Arc::new(
                names
                    .into_iter()
                    .map(|((_, model), name)| (model, name))
                    .collect(),
            );
        }
        let settings = history.settings().unwrap_or_default();
        app.zone = settings
            .get(TIME_ZONE_SETTING)
            .and_then(|name| TimeZone::get(name).ok())
            .unwrap_or_else(|| app.system_zone.clone());
        app.week_start = settings
            .get(WEEK_START_SETTING)
            .and_then(|name| weekday(name))
            .unwrap_or(Weekday::Monday);
        app.prices = price_refresh::in_use(&settings);
        app
    }

    /// The first day of the week `day` falls in.
    #[must_use]
    pub fn start_of_week(&self, day: Date) -> Date {
        Bucket::Week.start(day, self.week_start)
    }

    #[must_use]
    pub fn now_zoned(&self) -> Zoned {
        self.now.to_zoned(self.zone.clone())
    }

    /// The same app as it is at `now`, for work that outlives a moment, such as a server.
    #[must_use]
    pub fn at(&self, now: Timestamp) -> Self {
        Self {
            now,
            ..self.clone()
        }
    }

    /// Opens the history, creating it on first use.
    ///
    /// # Errors
    ///
    /// Why the history cannot be opened, or that this system has no folder for it.
    pub fn open_history(&self) -> Result<History, String> {
        let path = self.history_path.as_ref().ok_or_else(|| {
            format!(
                "There is no folder for local app data on this system; set {HISTORY_ENV} to a file for the history."
            )
        })?;
        History::open(path).map_err(|error| error.to_string())
    }

    /// Where Claude Code keeps the login that the usage endpoint needs.
    #[must_use]
    pub fn login_source(&self) -> Option<LoginSource> {
        LoginSource::of(&self.roots, self.config_dir.as_deref())
    }
}
