//! Task reminders: on a task's due day at the morning time, once more the morning after if it's
//! still open, and at a task's exact reminder time. Only on a computer someone is using; each
//! one sent is marked on its task, and the mark travels with the task's plan, so a combined
//! computer doesn't send it again.

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use tauri::AppHandle;

use crate::tasks::Task;

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

/// A computer counts as in use with input in the last two minutes.
const IN_USE_MS: i64 = 2 * 60_000;

pub fn spawn(app: AppHandle, db_path: PathBuf) {
    thread::Builder::new()
        .name("kairo-reminders".into())
        .spawn(move || {
            thread::sleep(Duration::from_secs(20));
            let conn = match crate::db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => return eprintln!("reminders: cannot open the database: {e}"),
            };
            loop {
                if let Err(e) = check(&app, &conn) {
                    eprintln!("reminders: {e}");
                }
                thread::sleep(Duration::from_secs(60));
            }
        })
        .expect("failed to start reminders thread");
}

fn check(app: &AppHandle, conn: &rusqlite::Connection) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    let settings = crate::ledger::settings(conn).map_err(|e| e.to_string())?;
    if !settings.reminders {
        return Ok(());
    }
    let morning = NaiveTime::parse_from_str(&settings.morning, "%H:%M").map_err(|e| e.to_string())?;
    let now = chrono::Local::now();
    let tasks = crate::tasks::list(conn, now.timestamp_millis(), &crate::tasks::today()).map_err(|e| e.to_string())?;
    let due = due_now(&tasks, now.timestamp_millis(), now.naive_local(), morning);
    let mut marks = due.drop;
    if crate::platform::idle_ms() < IN_USE_MS {
        for r in due.show {
            if let Err(e) = app.notification().builder().title(&r.title).body(&r.body).show() {
                eprintln!("reminders: {e}");
            }
            marks.extend(r.marks);
        }
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

    #[test]
    fn a_new_day_after_a_change_reminds_again() {
        let mut t = task("a", "Essay", Some(NOW_DAY), 1);
        // Sent for the day it used to be due.
        t.reminded = "due:2026-11-01".into();
        assert_eq!(titles(&due_now(&[t], 0, at(9, 0), morning())), ["Due today: Essay"]);
    }
}
