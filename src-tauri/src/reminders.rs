//! Task reminders: on a task's due day at the morning time, once more the morning after if it's
//! still open, at a task's exact reminder time, and at a moment: when an app or site comes to the
//! front, or a focus session ends. Only on a computer someone is using; each one sent is marked on
//! its task, and the mark travels with the task's plan, so a combined computer doesn't send it
//! again.

use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use tauri::AppHandle;

use crate::tasks::Task;

/// What is in front, as a moment reminder can name it. Only what the collector records: an app or
/// site the user ignores never gets here, and a window whose title is hidden has none.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Front {
    pub app: String,
    pub exe: String,
    pub title: String,
    pub site: Option<String>,
}

/// A moment a task can be reminded at.
#[derive(Debug, Clone, PartialEq)]
pub enum Moment {
    /// An app or site came to the front.
    Front(Front),
    /// A focus session ended, on its own or early.
    FocusEnded,
}

/// Where moments go: the reminders thread, once it has started.
static MOMENTS: OnceLock<Sender<Moment>> = OnceLock::new();

/// Tells the reminders thread a moment has come; nothing happens before it has started.
pub fn announce(moment: Moment) {
    if let Some(tx) = MOMENTS.get() {
        let _ = tx.send(moment);
    }
}

/// What to show, and the marks it leaves on its tasks.
#[derive(Debug, Clone, PartialEq)]
pub struct Reminder {
    pub title: String,
    pub body: String,
    /// (task id, mark)
    pub marks: Vec<(String, String)>,
}

/// What's due now: reminders to show, and marks for ones too late to show, left unshown.
#[derive(Debug, Default, PartialEq)]
pub struct Due {
    pub show: Vec<Reminder>,
    pub drop: Vec<(String, String)>,
}

/// Later than this, a reminder isn't worth showing.
const LATE_MS: i64 = 24 * 60 * 60 * 1000;

fn sent(t: &Task, mark: &str) -> bool {
    t.reminded.split(' ').any(|m| m == mark)
}

fn day_text(day: &str) -> Option<String> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").ok().map(|d| d.format("%a %-d %b").to_string())
}

/// One notification for every task in `tasks`, named by the first: "Due today: Essay and 2 more".
fn gathered(what: &str, tasks: &[&Task], mark: impl Fn(&Task) -> String) -> Option<Reminder> {
    let first = tasks.first()?;
    let title = match tasks.len() {
        1 => format!("{what}: {}", first.title),
        n => format!("{what}: {} and {} more", first.title, n - 1),
    };
    let body = tasks[1..].iter().map(|t| t.title.as_str()).collect::<Vec<_>>().join(", ");
    Some(Reminder { title, body, marks: tasks.iter().map(|t| (t.id.clone(), mark(t))).collect() })
}

/// High first; a stable sort, so ties keep Ledger's order.
fn by_priority(mut tasks: Vec<&Task>) -> Vec<&Task> {
    tasks.sort_by_key(|t| std::cmp::Reverse(t.priority));
    tasks
}

/// The reminders due at `now` (`now_local` the same moment on this computer's clock), for
/// `tasks` in Ledger's order. Pure: nothing is sent or marked.
pub fn due_now(tasks: &[Task], now_ms: i64, now_local: NaiveDateTime, morning: NaiveTime) -> Due {
    let today = now_local.date();
    let (today_text, yesterday_text) = (today.format("%Y-%m-%d").to_string(), today.pred_opt().map(|d| d.format("%Y-%m-%d").to_string()));
    let open: Vec<&Task> = tasks.iter().filter(|t| t.done_ms.is_none()).collect();
    let after_morning = now_local.time() >= morning;
    let mut due = Due::default();
    if after_morning {
        let today_due = by_priority(open.iter().copied().filter(|t| t.due_day.as_deref() == Some(&today_text) && !sent(t, &format!("due:{today_text}"))).collect());
        due.show.extend(gathered("Due today", &today_due, |t| format!("due:{}", t.due_day.as_deref().unwrap_or(""))));
        let late = by_priority(
            open.iter().copied().filter(|t| t.due_day.is_some() && t.due_day == yesterday_text && !sent(t, &format!("over:{}", t.due_day.as_deref().unwrap_or("")))).collect(),
        );
        due.show.extend(gathered("Overdue", &late, |t| format!("over:{}", t.due_day.as_deref().unwrap_or(""))));
    }
    for t in &open {
        let Some(at) = t.remind_ms.filter(|at| *at <= now_ms) else { continue };
        let mark = format!("at:{at}");
        if sent(t, &mark) {
            continue;
        }
        if now_ms - at > LATE_MS {
            due.drop.push((t.id.clone(), mark));
            continue;
        }
        let body = t.due_day.as_deref().and_then(day_text).map(|d| format!("Due {d}")).unwrap_or_default();
        due.show.push(Reminder { title: format!("Reminder: {}", t.title), body, marks: vec![(t.id.clone(), mark)] });
    }
    due
}

/// The hour from which the day is wrapped up, not planned: the nudge stops then.
const EVENING_FROM_HOUR: u32 = 17;

/// "2h 30m" for minutes.
fn minutes_name(min: i64) -> String {
    match (min / 60, min % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// The "Plan the day?" nudge: its title and body, when it is time. From the morning time until
/// five, once a day (`last` is the day it last came), when something is due today or overdue.
/// Pure: nothing is shown or marked.
pub fn plan_nudge(tasks: &[Task], now_local: NaiveDateTime, morning: NaiveTime, last: Option<&str>) -> Option<(String, String)> {
    use chrono::Timelike;
    let today = now_local.date().format("%Y-%m-%d").to_string();
    if now_local.time() < morning || now_local.hour() >= EVENING_FROM_HOUR || last == Some(today.as_str()) {
        return None;
    }
    let left: Vec<&Task> = tasks.iter().filter(|t| t.done_ms.is_none() && matches!(t.group.as_str(), "overdue" | "today")).collect();
    if left.is_empty() {
        return None;
    }
    let planned: i64 = left.iter().filter_map(|t| t.estimate_min).sum();
    let count = format!("{} to do today", left.len());
    let body = if planned > 0 { format!("{count} · {} planned", minutes_name(planned)) } else { count };
    Some(("Plan the day?".into(), body))
}

/// The "Wrap up the day?" nudge: its title and body, when it is time. From the end of the working
/// day (never before five, when Plan turns to wrapping up) until eleven, once a day, when something
/// was done today or is still left. Pure: nothing is shown or marked.
pub fn wrap_nudge(tasks: &[Task], now_local: NaiveDateTime, day_start_ms: i64, day_to: NaiveTime, last: Option<&str>) -> Option<(String, String)> {
    use chrono::Timelike;
    let today = now_local.date().format("%Y-%m-%d").to_string();
    let from = day_to.max(NaiveTime::from_hms_opt(EVENING_FROM_HOUR, 0, 0).expect("an hour"));
    if now_local.time() < from || now_local.hour() >= WRAP_UNTIL_HOUR || last == Some(today.as_str()) {
        return None;
    }
    let left = tasks.iter().filter(|t| t.done_ms.is_none() && matches!(t.group.as_str(), "overdue" | "today")).count();
    let done = tasks.iter().filter(|t| t.done_ms.is_some_and(|d| d >= day_start_ms)).count();
    if left == 0 && done == 0 {
        return None;
    }
    let body = match (done, left) {
        (0, l) => format!("{l} left"),
        (d, 0) => format!("{d} done · everything due is done"),
        (d, l) => format!("{d} done · {l} left"),
    };
    Some(("Wrap up the day?".into(), body))
}

/// The wrap-up nudge isn't sent this late: a late return isn't the end of a working day.
const WRAP_UNTIL_HOUR: u32 = 23;

/// The mark a moment reminder leaves: its moment, with no spaces (marks are space-separated).
pub fn when_mark(when: &str) -> String {
    format!("when:{}", when.replace(' ', "_"))
}

/// Whether `front` is what `trigger` (`open:<words>`) is waiting for: any of its comma-separated
/// words in the app's name, its program, the site or the window's title, whatever the case.
fn watching(trigger: &str, front: &Front) -> bool {
    let Some(words) = trigger.strip_prefix("open:") else { return false };
    let seen = [front.app.as_str(), front.exe.as_str(), front.site.as_deref().unwrap_or(""), front.title.as_str()].map(str::to_lowercase);
    words.split(',').map(str::trim).filter(|w| !w.is_empty()).any(|w| seen.iter().any(|s| s.contains(&w.to_lowercase())))
}

/// The reminders a moment brings, one for each open task waiting for it that hasn't sent it yet.
/// Pure: nothing is sent or marked.
pub fn moment_due(tasks: &[Task], moment: &Moment) -> Vec<Reminder> {
    tasks
        .iter()
        .filter(|t| t.done_ms.is_none())
        .filter_map(|t| {
            let when = t.remind_when.as_deref()?;
            let mark = when_mark(when);
            if sent(t, &mark) {
                return None;
            }
            let body = match moment {
                Moment::Front(front) if watching(when, front) => {
                    let what = front.site.as_deref().filter(|s| !s.is_empty()).unwrap_or(front.app.as_str());
                    format!("You opened {what}")
                }
                Moment::FocusEnded if when == "focus" => "Your focus session ended".to_string(),
                _ => return None,
            };
            Some(Reminder { title: format!("Reminder: {}", t.title), body, marks: vec![(t.id.clone(), mark)] })
        })
        .collect()
}

/// A computer counts as in use with input in the last two minutes.
const IN_USE_MS: i64 = 2 * 60_000;

/// How often the clock-based reminders are looked at.
const CHECK_EVERY: Duration = Duration::from_secs(60);

/// The extra a notification carries to say where a click on it goes: the Plan nudge's opens
/// Ledger on Plan.
pub const OPENS: &str = "kairoOpens";

/// The extra a one-task reminder carries: the task, by id, for its Start focus button.
const TASK: &str = "kairoTask";
/// The buttons a one-task reminder has, as registered with the notifications.
const TASK_BUTTONS: &str = "kairo-task";
/// The Start focus button's action, and the session it starts.
const START_FOCUS: &str = "focus";
pub const REMINDER_FOCUS_MINUTES: u32 = 25;

/// What a click on one of Kairo's notifications does.
#[derive(Debug, PartialEq, Eq)]
pub enum Clicked {
    /// Opens Ledger there ("plan", "task:<id>", "note:<id>").
    Open(String),
    /// Starts a focus session on the task, by id: a reminder's Start focus button.
    Focus(String),
}

/// What a click on a notification asks for: the notification itself opens where it says, its
/// Start focus button starts a session on its task; anything else, nothing.
pub fn clicked(performed: &tauri_plugin_notification::ActionPerformed) -> Option<Clicked> {
    let extra = performed.notification()?.extra();
    let said = |key: &str| extra.get(key).and_then(|v| v.as_str()).map(str::to_string);
    match performed.action_id() {
        "tap" => said(OPENS).map(Clicked::Open),
        START_FOCUS => said(TASK).map(Clicked::Focus),
        _ => None,
    }
}

/// The task a reminder is for, when it's for one alone: only then does it open on it and offer
/// to start a session on it.
pub fn single_task(r: &Reminder) -> Option<&str> {
    let first = r.marks.first()?.0.as_str();
    r.marks.iter().all(|(id, _)| id == first).then_some(first)
}

/// Opens Ledger where a clicked notification says, or starts a focus session from a reminder's
/// button. Windows reports a click only while the toast is on screen, not from the Action Center;
/// there the notification only tells.
pub fn follow_clicks(app: &AppHandle) {
    use tauri_plugin_notification::{Action, ActionType, NotificationExt};
    let start = Action::builder(START_FOCUS, format!("Start focus ({REMINDER_FOCUS_MINUTES} min)")).build();
    let _ = app.notification().register_action_types(vec![ActionType::builder(TASK_BUTTONS).actions(vec![start]).build()]);
    let handle = app.clone();
    let _ = app.notification().on_action(move |performed| match clicked(performed) {
        Some(Clicked::Open(at)) => {
            let app = handle.clone();
            let _ = handle.run_on_main_thread(move || crate::show_ledger_at(&app, &at));
        }
        Some(Clicked::Focus(task)) => start_focus_on(&handle, &task),
        None => {}
    });
}

/// A reminder's Start focus: a session on its task, unless one is running already (it isn't
/// ended from a notification), said in a notification when it can't start.
fn start_focus_on(app: &AppHandle, task: &str) {
    use tauri::{Emitter, Manager};
    let Some(state) = app.try_state::<crate::AppState>() else { return };
    let started = {
        let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
        match crate::focus::active(&conn) {
            Ok(Some(_)) => Err("A focus session is already running."),
            Ok(None) => crate::focus::start_by_hand(&conn, REMINDER_FOCUS_MINUTES, None, Some(task), chrono::Utc::now().timestamp_millis()).map(|_| ()).map_err(|_| "Couldn't start a focus session. Try again from the palette."),
            Err(_) => Err("Couldn't start a focus session. Try again from the palette."),
        }
    };
    match started {
        Ok(()) => {
            let _ = app.emit("kairo:focus", ());
        }
        Err(why) => crate::capture::tell(app, "Kairo", why, None),
    }
}

pub fn spawn(app: AppHandle, db_path: PathBuf) {
    let (tx, moments) = mpsc::channel();
    let _ = MOMENTS.set(tx);
    thread::Builder::new()
        .name("kairo-reminders".into())
        .spawn(move || {
            thread::sleep(Duration::from_secs(20));
            let conn = match crate::db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => return eprintln!("reminders: cannot open the database: {e}"),
            };
            // Waits for a moment, and looks at the clock-based ones every minute whatever comes.
            let mut next_check = Instant::now();
            loop {
                let wait = next_check.saturating_duration_since(Instant::now());
                match moments.recv_timeout(wait) {
                    Ok(moment) => {
                        if let Err(e) = on_moment(&app, &conn, &moment) {
                            eprintln!("reminders: {e}");
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => thread::sleep(wait),
                }
                if Instant::now() >= next_check {
                    if let Err(e) = check(&app, &conn) {
                        eprintln!("reminders: {e}");
                    }
                    next_check = Instant::now() + CHECK_EVERY;
                }
            }
        })
        .expect("failed to start reminders thread");
}

fn check(app: &AppHandle, conn: &rusqlite::Connection) -> Result<(), String> {
    let settings = crate::ledger::settings(conn).map_err(|e| e.to_string())?;
    if !settings.reminders {
        return Ok(());
    }
    let morning = NaiveTime::parse_from_str(&settings.morning, "%H:%M").map_err(|e| e.to_string())?;
    let now = chrono::Local::now();
    let tasks = crate::tasks::list(conn, now.timestamp_millis(), &crate::tasks::today()).map_err(|e| e.to_string())?;
    let due = due_now(&tasks, now.timestamp_millis(), now.naive_local(), morning);
    let in_use = crate::platform::idle_ms() < IN_USE_MS;
    let show = if in_use { due.show } else { Vec::new() };
    if settings.plan_nudge && in_use {
        let last = crate::ledger::plan_nudged(conn).map_err(|e| e.to_string())?;
        if let Some((title, body)) = plan_nudge(&tasks, now.naive_local(), morning, last.as_deref()) {
            use tauri_plugin_notification::NotificationExt;
            // A click on it opens Ledger on Plan (follow_clicks).
            if let Err(e) = app.notification().builder().title(&title).body(&body).extra(OPENS, "plan").show() {
                eprintln!("reminders: {e}");
            }
            // Marked for every computer, so a combined one doesn't ask twice.
            let today = now.format("%Y-%m-%d").to_string();
            crate::ledger::set_plan_nudged(conn, &today).map_err(|e| e.to_string())?;
            crate::sync::record(conn, "ledger:plan_nudged", &today)?;
        }
    }
    if settings.wrap_nudge && in_use {
        let day_to = NaiveTime::parse_from_str(&settings.day_to, "%H:%M").map_err(|e| e.to_string())?;
        let start = crate::analysis::day_bounds(now.date_naive()).map(|(from, _)| from).unwrap_or(0);
        let last = crate::ledger::wrap_nudged(conn).map_err(|e| e.to_string())?;
        if let Some((title, body)) = wrap_nudge(&tasks, now.naive_local(), start, day_to, last.as_deref()) {
            use tauri_plugin_notification::NotificationExt;
            // A click opens Plan, which is wrapping the day up by now.
            if let Err(e) = app.notification().builder().title(&title).body(&body).extra(OPENS, "plan").show() {
                eprintln!("reminders: {e}");
            }
            let today = now.format("%Y-%m-%d").to_string();
            crate::ledger::set_wrap_nudged(conn, &today).map_err(|e| e.to_string())?;
            crate::sync::record(conn, "ledger:wrap_nudged", &today)?;
        }
    }
    send(app, conn, show, due.drop)
}

/// A moment has come: the reminders waiting for it, if reminders are on.
fn on_moment(app: &AppHandle, conn: &rusqlite::Connection, moment: &Moment) -> Result<(), String> {
    if !crate::ledger::settings(conn).map_err(|e| e.to_string())?.reminders {
        return Ok(());
    }
    let now = chrono::Local::now();
    let tasks = crate::tasks::list(conn, now.timestamp_millis(), &crate::tasks::today()).map_err(|e| e.to_string())?;
    send(app, conn, moment_due(&tasks, moment), Vec::new())
}

/// Shows `show`, and marks it and `marks` sent on their tasks, sharing the marks with combined
/// computers.
fn send(app: &AppHandle, conn: &rusqlite::Connection, show: Vec<Reminder>, mut marks: Vec<(String, String)>) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    for r in show {
        let mut n = app.notification().builder().title(&r.title).body(&r.body);
        // One task's reminder opens it when clicked, and offers to start a session on it.
        if let Some(id) = single_task(&r) {
            n = n.extra(OPENS, format!("task:{id}")).extra(TASK, id).action_type_id(TASK_BUTTONS);
        }
        if let Err(e) = n.show() {
            eprintln!("reminders: {e}");
        }
        marks.extend(r.marks);
    }
    for (id, mark) in &marks {
        crate::tasks::mark_sent(conn, id, mark).map_err(|e| e.to_string())?;
    }
    let mut ids: Vec<&String> = marks.iter().map(|(id, _)| id).collect();
    ids.dedup();
    for id in ids {
        if let Some(plan) = crate::tasks::plan_json(conn, id).map_err(|e| e.to_string())? {
            crate::sync::record(conn, &format!("taskplan:{id}"), &plan)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_DAY: &str = "2026-11-03";

    /// What the notification plugin hands its handlers: the action, and the notification as it was built.
    fn click(action: &str, notification: serde_json::Value) -> tauri_plugin_notification::ActionPerformed {
        serde_json::from_value(serde_json::json!({ "actionId": action, "inputValue": null, "notification": notification })).unwrap()
    }

    #[test]
    fn a_click_opens_where_it_says_and_a_reminders_button_starts_focus_on_its_task() {
        let nudge = serde_json::json!({ "id": 7, "title": "Plan the day?", "extra": { OPENS: "plan" } });
        assert_eq!(clicked(&click("tap", nudge.clone())), Some(Clicked::Open("plan".into())));
        // A button the notification doesn't have.
        assert_eq!(clicked(&click("snooze", nudge)), None);
        let reminder = serde_json::json!({ "id": 8, "title": "Due today: Essay", "extra": { OPENS: "task:t1", TASK: "t1" } });
        assert_eq!(clicked(&click("tap", reminder.clone())), Some(Clicked::Open("task:t1".into())));
        assert_eq!(clicked(&click(START_FOCUS, reminder)), Some(Clicked::Focus("t1".into())));
        // A reminder for several tasks, or one the platform couldn't describe.
        assert_eq!(clicked(&click("tap", serde_json::json!({ "id": 9, "title": "Due today: Essay and 2 more" }))), None);
        assert_eq!(clicked(&click("tap", serde_json::Value::Null)), None);
    }

    #[test]
    fn only_a_reminder_for_one_task_is_tied_to_it() {
        let one = Reminder { title: "Reminder: Essay".into(), body: String::new(), marks: vec![("t1".into(), "due".into()), ("t1".into(), "morning".into())] };
        assert_eq!(single_task(&one), Some("t1"));
        let many = Reminder { marks: vec![("t1".into(), "due".into()), ("t2".into(), "due".into())], ..one };
        assert_eq!(single_task(&many), None);
    }

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 3).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    fn morning() -> NaiveTime {
        NaiveTime::from_hms_opt(9, 0, 0).unwrap()
    }

    fn task(id: &str, title: &str, due: Option<&str>, priority: u8) -> Task {
        Task { id: id.into(), title: title.into(), due_day: due.map(Into::into), priority, ..Default::default() }
    }

    fn titles(d: &Due) -> Vec<&str> {
        d.show.iter().map(|r| r.title.as_str()).collect()
    }

    #[test]
    fn the_wrap_up_nudge_comes_once_at_the_end_of_the_working_day_never_before_five() {
        let start = 1_000;
        let mut left = task("a", "Essay", Some(NOW_DAY), 1);
        left.group = "today".into();
        let mut done = task("b", "Call", Some(NOW_DAY), 1);
        done.group = "done".into();
        done.done_ms = Some(start + 5);
        let mut old = task("c", "Old", None, 1);
        old.group = "done".into();
        old.done_ms = Some(start - 5);
        let six = NaiveTime::from_hms_opt(18, 0, 0).unwrap();
        let tasks = [left.clone(), done.clone(), old.clone()];
        let nudge = |h, m, to: NaiveTime, last: Option<&str>| wrap_nudge(&tasks, at(h, m), start, to, last);
        assert_eq!(nudge(18, 0, six, None), Some(("Wrap up the day?".to_string(), "1 done · 1 left".to_string())));
        assert_eq!(nudge(17, 59, six, None), None);
        // A working day ending at four still waits for five, when Plan wraps the day up.
        let four = NaiveTime::from_hms_opt(16, 0, 0).unwrap();
        assert_eq!(nudge(16, 30, four, None), None);
        assert!(nudge(17, 0, four, None).is_some());
        // Not late at night, and once a day.
        assert_eq!(nudge(23, 0, six, None), None);
        assert_eq!(nudge(19, 0, six, Some(NOW_DAY)), None);
        // What it says: all done; only left; nothing at all is nothing to wrap up.
        assert_eq!(wrap_nudge(&[done.clone()], at(18, 0), start, six, None).unwrap().1, "1 done · everything due is done");
        assert_eq!(wrap_nudge(&[left], at(18, 0), start, six, None).unwrap().1, "1 left");
        assert_eq!(wrap_nudge(&[old], at(18, 0), start, six, None), None);
    }

    #[test]
    fn the_plan_nudge_comes_once_a_day_between_the_morning_and_five_when_something_is_due() {
        let mut today = task("a", "Essay", Some(NOW_DAY), 1);
        today.group = "today".into();
        today.estimate_min = Some(90);
        let mut late = task("b", "Call", Some("2026-11-02"), 1);
        late.group = "overdue".into();
        let mut later = task("c", "Someday", Some("2026-11-20"), 1);
        later.group = "upcoming".into();
        let tasks = [today.clone(), late, later];
        let nudge = |h, m, last: Option<&str>| plan_nudge(&tasks, at(h, m), morning(), last);
        assert_eq!(nudge(9, 0, None), Some(("Plan the day?".to_string(), "2 to do today · 1h 30m planned".to_string())));
        assert_eq!(nudge(8, 59, None), None);
        // Opened late in the morning, it still comes; from five on the day is wrapped up instead.
        assert!(nudge(13, 0, None).is_some());
        assert_eq!(nudge(17, 0, None), None);
        // Once a day, whichever computer it came on.
        assert_eq!(nudge(10, 0, Some("2026-11-03")), None);
        assert!(nudge(10, 0, Some("2026-11-02")).is_some());
        // Nothing due, or all done: nothing to plan.
        assert_eq!(plan_nudge(&[tasks[2].clone()], at(10, 0), morning(), None), None);
        today.done_ms = Some(1);
        assert_eq!(plan_nudge(&[today], at(10, 0), morning(), None), None);
    }

    #[test]
    fn due_today_at_the_morning_time_not_before() {
        let tasks = [task("a", "Essay", Some(NOW_DAY), 1)];
        assert!(due_now(&tasks, 0, at(8, 59), morning()).show.is_empty());
        let d = due_now(&tasks, 0, at(9, 0), morning());
        assert_eq!(titles(&d), ["Due today: Essay"]);
        assert_eq!(d.show[0].marks, [("a".to_string(), "due:2026-11-03".to_string())]);
        // Later in the day, after being away, it still comes.
        assert_eq!(titles(&due_now(&tasks, 0, at(21, 30), morning())), ["Due today: Essay"]);
    }

    #[test]
    fn several_due_come_in_one_named_by_the_most_important() {
        let tasks = [task("a", "Laundry", Some(NOW_DAY), 0), task("b", "Essay", Some(NOW_DAY), 2), task("c", "Call", Some(NOW_DAY), 1)];
        let d = due_now(&tasks, 0, at(9, 5), morning());
        assert_eq!(titles(&d), ["Due today: Essay and 2 more"]);
        assert_eq!(d.show[0].body, "Call, Laundry");
        assert_eq!(d.show[0].marks.len(), 3);
    }

    #[test]
    fn overdue_the_morning_after_once() {
        let mut t = task("a", "Essay", Some("2026-11-02"), 1);
        assert_eq!(titles(&due_now(&[t.clone()], 0, at(9, 0), morning())), ["Overdue: Essay"]);
        t.reminded = "over:2026-11-02".into();
        assert!(due_now(&[t.clone()], 0, at(9, 0), morning()).show.is_empty());
        // Two days late: no more.
        let older = task("b", "Older", Some("2026-11-01"), 1);
        assert!(due_now(&[older], 0, at(9, 0), morning()).show.is_empty());
    }

    #[test]
    fn an_exact_reminder_comes_at_its_time_with_its_day() {
        let mut t = task("a", "Essay", Some("2026-11-06"), 1);
        t.remind_ms = Some(1_000);
        assert!(due_now(&[t.clone()], 999, at(7, 0), morning()).show.is_empty());
        let d = due_now(&[t.clone()], 1_000, at(7, 0), morning());
        assert_eq!(titles(&d), ["Reminder: Essay"]);
        assert_eq!(d.show[0].body, "Due Fri 6 Nov");
        assert_eq!(d.show[0].marks, [("a".to_string(), "at:1000".to_string())]);
        t.reminded = "at:1000".into();
        assert!(due_now(&[t], 5_000, at(7, 0), morning()).show.is_empty());
    }

    #[test]
    fn a_reminder_a_day_late_is_dropped_unshown() {
        let mut t = task("a", "Essay", None, 1);
        t.remind_ms = Some(0);
        let d = due_now(&[t], LATE_MS + 1, at(10, 0), morning());
        assert!(d.show.is_empty());
        assert_eq!(d.drop, [("a".to_string(), "at:0".to_string())]);
    }

    #[test]
    fn sent_and_done_tasks_never_remind() {
        let mut sent_one = task("a", "Essay", Some(NOW_DAY), 1);
        sent_one.reminded = "due:2026-11-03".into();
        let mut done = task("b", "Done", Some(NOW_DAY), 1);
        done.done_ms = Some(5);
        done.remind_ms = Some(0);
        assert_eq!(due_now(&[sent_one, done], 10, at(9, 30), morning()), Due::default());
    }

    fn waiting(id: &str, title: &str, when: &str) -> Task {
        Task { id: id.into(), title: title.into(), remind_when: Some(when.into()), ..Default::default() }
    }

    fn front(app: &str, site: Option<&str>, title: &str) -> Moment {
        Moment::Front(Front { app: app.into(), exe: app.to_lowercase(), title: title.into(), site: site.map(Into::into) })
    }

    #[test]
    fn a_site_coming_to_the_front_reminds_the_task_waiting_for_it() {
        let tasks = [waiting("a", "Submit essay", "open:gradescope"), waiting("b", "Call", "open:slack")];
        let r = moment_due(&tasks, &front("Chrome", Some("gradescope.com"), "Assignments"));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "Reminder: Submit essay");
        assert_eq!(r[0].body, "You opened gradescope.com");
        assert_eq!(r[0].marks, [("a".to_string(), "when:open:gradescope".to_string())]);
        // Another site, or an app that isn't it, says nothing.
        assert!(moment_due(&tasks, &front("Chrome", Some("youtube.com"), "Lofi")).is_empty());
    }

    #[test]
    fn words_match_the_app_program_site_or_title_in_any_case_and_any_of_several() {
        let t = |when| [waiting("a", "T", when)];
        assert_eq!(moment_due(&t("open:WORD"), &front("Word", None, "")).len(), 1);
        assert_eq!(moment_due(&t("open:code"), &front("Visual Studio Code", None, "")).len(), 1);
        assert_eq!(moment_due(&t("open:thesis"), &front("Word", None, "Thesis draft.docx - Word")).len(), 1);
        assert_eq!(moment_due(&t("open:figma, notion"), &front("Notion", None, "")).len(), 1);
        // A hidden title gives nothing to match.
        assert!(moment_due(&t("open:thesis"), &front("Word", None, "")).is_empty());
        // No words, no match, and a focus reminder isn't waiting for the front.
        assert!(moment_due(&t("open:"), &front("Word", None, "")).is_empty());
        assert!(moment_due(&t("focus"), &front("Word", None, "")).is_empty());
    }

    #[test]
    fn a_focus_session_ending_reminds_only_tasks_waiting_for_it() {
        let tasks = [waiting("a", "Stretch", "focus"), waiting("b", "Essay", "open:word")];
        let r = moment_due(&tasks, &Moment::FocusEnded);
        assert_eq!(r.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["Reminder: Stretch"]);
        assert_eq!(r[0].body, "Your focus session ended");
    }

    #[test]
    fn a_moment_reminder_comes_once_and_never_for_a_done_task() {
        let mut once = waiting("a", "Essay", "open:word");
        once.reminded = "due:2026-11-03 when:open:word".into();
        let mut done = waiting("b", "Done", "focus");
        done.done_ms = Some(5);
        assert!(moment_due(&[once, done], &front("Word", None, "")).is_empty());
        // A moment with a space leaves a mark without one, so the marks stay separable.
        assert_eq!(when_mark("open:google docs"), "when:open:google_docs");
        let mut spaced = waiting("c", "Doc", "open:google docs");
        spaced.reminded = when_mark("open:google docs");
        assert!(moment_due(&[spaced], &front("Chrome", Some("docs.google.com"), "Google Docs")).is_empty());
    }

    #[test]
    fn a_new_day_after_a_change_reminds_again() {
        let mut t = task("a", "Essay", Some(NOW_DAY), 1);
        // Sent for the day it used to be due.
        t.reminded = "due:2026-11-01".into();
        assert_eq!(titles(&due_now(&[t], 0, at(9, 0), morning())), ["Due today: Essay"]);
    }
}
