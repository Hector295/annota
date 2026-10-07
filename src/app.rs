//! Application lifecycle: owns the `gtk::Application` and wires capture →
//! overlay/editor → clipboard/export together.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{Application, gio, glib};

use crate::capture::{self, CaptureError, ScreenshotBackend};
use crate::ui::Session;

/// Reverse-DNS id. Used for D-Bus single instance, the portal and Flatpak.
pub const APP_ID: &str = "io.github.hector295.Annota";

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Capturing,
    #[cfg(target_os = "linux")]
    AskingPermission(gtk::ApplicationWindow),
    Editing(Rc<Session>),
}

type SharedState = Rc<RefCell<State>>;

/// Longest accepted `--delay`.
const MAX_DELAY_MS: i32 = 10_000;

pub fn run() -> glib::ExitCode {
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    app.add_main_option(
        "delay",
        glib::Char::from(b'd'),
        glib::OptionFlags::NONE,
        glib::OptionArg::Int,
        "Wait MS milliseconds before capturing (e.g. while a menu closes)",
        Some("MS"),
    );
    // Launching `annota` again (e.g. from a keyboard shortcut) forwards the
    // command line to the running instance, which may be alive serving the
    // clipboard.
    let state = SharedState::default();
    app.connect_command_line(move |app, command_line| {
        let delay = command_line
            .options_dict()
            .lookup::<i32>("delay")
            .ok()
            .flatten()
            .unwrap_or(0)
            .clamp(0, MAX_DELAY_MS);
        activate(app, &state, Duration::from_millis(delay as u64));
        glib::ExitCode::SUCCESS
    });
    app.run()
}

fn activate(app: &Application, state: &SharedState, delay: Duration) {
    match &*state.borrow() {
        State::Capturing => return,
        #[cfg(target_os = "linux")]
        State::AskingPermission(window) => return window.present(),
        State::Editing(session) => return session.present(),
        State::Idle => {}
    }
    start_capture(app, state, delay);
}

fn start_capture(app: &Application, state: &SharedState, delay: Duration) {
    *state.borrow_mut() = State::Capturing;
    // No window exists while the portal runs; keep the app alive meanwhile.
    let hold = app.hold();
    let (app, state) = (app.clone(), state.clone());
    glib::spawn_future_local(async move {
        let _hold = hold;
        if !delay.is_zero() {
            glib::timeout_future(delay).await;
        }
        let result = capture::default_backend().capture().await;
        *state.borrow_mut() = State::Idle;
        let image = match result {
            Ok(image) => image,
            Err(CaptureError::Cancelled) => return,
            Err(CaptureError::Refused) => return on_refused(&app, &state),
            Err(CaptureError::Failed(err)) => return eprintln!("annota: {err:#}"),
        };
        let reset = state.clone();
        match Session::open(&app, image, move || *reset.borrow_mut() = State::Idle) {
            Ok(session) => *state.borrow_mut() = State::Editing(session),
            Err(err) => eprintln!("annota: {err:#}"),
        }
    });
}

/// The system refused to capture. On GNOME that usually means the app has
/// no screenshot permission yet: offer the first-run permission window.
#[cfg(target_os = "linux")]
fn on_refused(app: &Application, state: &SharedState) {
    use crate::ui::permission;

    if let Some(scope) = capture::foreign_scope() {
        eprintln!(
            "annota: the portal refused the capture. This process runs inside {scope}, \
             so GNOME attributes it to that app. Run annota from a normal terminal \
             or from the applications menu."
        );
    }
    let (granted_app, granted_state) = (app.clone(), state.clone());
    let window = permission::show(app, move || {
        start_capture(&granted_app, &granted_state, Duration::ZERO)
    });
    let closed = state.clone();
    window.connect_close_request(move |_| {
        if matches!(*closed.borrow(), State::AskingPermission(_)) {
            *closed.borrow_mut() = State::Idle;
        }
        glib::Propagation::Proceed
    });
    *state.borrow_mut() = State::AskingPermission(window);
}

#[cfg(not(target_os = "linux"))]
fn on_refused(_: &Application, _: &SharedState) {
    eprintln!("annota: the system refused to capture the screen");
}
