//! Turns raw sessions into daily summaries, hour-of-day patterns, and plain-language insights.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike};
use rusqlite::{params, Connection, Result};
use serde::Serialize;

const MIN_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MIN_MS;
/// Shorter visits are alt-tab pass-throughs, not real switches.
const MIN_VISIT_MS: i64 = 2_000;
/// A focus block survives short interruptions but not real breaks.
const FOCUS_MAX_GAP_MS: i64 = 3 * MIN_MS;
const FOCUS_MAX_DISTRACTION_MS: i64 = MIN_MS;
const DEEP_BLOCK_MS: i64 = 25 * MIN_MS;
/// A day only counts toward patterns if it has this much activity.
const MIN_DAY_ACTIVE_MS: i64 = 10 * MIN_MS;
const MIN_DAYS_FOR_INSIGHTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Productive,
    Neutral,
    Distracting,
}

impl Category {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "productive" => Some(Self::Productive),
            "neutral" => Some(Self::Neutral),
            "distracting" => Some(Self::Distracting),
            _ => None,
        }
    }
}

fn local_midnight_ms(date: NaiveDate) -> Option<i64> {
    date.and_hms_opt(0, 0, 0)?.and_local_timezone(Local).earliest().map(|t| t.timestamp_millis())
}

/// Local midnight to the next local midnight, so DST days come out 23 or 25 hours long.
pub fn day_bounds(date: NaiveDate) -> Option<(i64, i64)> {
    Some((local_midnight_ms(date)?, local_midnight_ms(date.succ_opt()?)?))
}

pub struct Row {
    pub app: String,
    pub exe: String,
    pub site: Option<String>,
    pub start: i64,
    pub end: i64,
    pub idle: bool,
    /// Category of what was on screen (the site when known, otherwise the app).
    pub category: Category,
    /// Category of the app itself, for per-app totals.
    pub app_category: Category,
}

impl Row {
    pub fn dur(&self) -> i64 {
        self.end - self.start
    }

    /// What the user was "on": the site when we know it, otherwise the app.
    fn label(&self) -> &str {
        self.site.as_deref().unwrap_or(&self.app)
    }
}

pub fn category_map(conn: &Connection) -> Result<HashMap<String, Category>> {
    let mut stmt = conn.prepare("SELECT key, category FROM categories")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut map = HashMap::new();
    for row in rows {
        let (key, category) = row?;
        if let Some(category) = Category::parse(&category) {
            map.insert(key, category);
        }
    }
    Ok(map)
}

pub fn resolve(cats: &HashMap<String, Category>, exe: &str, site: Option<&str>) -> Category {
    site.and_then(|s| cats.get(&format!("site:{s}")))
        .or_else(|| cats.get(&format!("app:{exe}")))
        .copied()
        .unwrap_or(Category::Neutral)
}

/// Sessions overlapping [from, to), clipped to that range and sorted by start.
pub fn load_rows(conn: &Connection, from: i64, to: i64) -> Result<Vec<Row>> {
    let cats = category_map(conn)?;
    let mut stmt = conn.prepare(
        "SELECT app, exe, site, start_ms, end_ms, idle FROM sessions
         WHERE end_ms > ?1 AND start_ms < ?2 ORDER BY start_ms",
    )?;
    let rows = stmt.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, bool>(5)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (app, exe, site, start, end, idle) = row?;
        let (start, end) = (start.max(from), end.min(to));
        if end <= start {
            continue;
        }
        out.push(Row {
            category: resolve(&cats, &exe, site.as_deref()),
            app_category: resolve(&cats, &exe, None),
            app,
            exe,
            site,
            start,
            end,
            idle,
        });
    }
    Ok(out)
}

// ---------- building blocks ----------

fn local(ms: i64) -> DateTime<Local> {
    DateTime::from_timestamp_millis(ms).unwrap_or_default().with_timezone(&Local)
}

/// Split [start, end) at local hour boundaries.
fn split_by_hour(start: i64, end: i64) -> impl Iterator<Item = (i64, i64)> {
    let mut t = start;
    std::iter::from_fn(move || {
        if t >= end {
            return None;
        }
        let dt = local(t);
        let into_hour = (dt.minute() as i64 * 60 + dt.second() as i64) * 1000 + dt.timestamp_subsec_millis() as i64;
        let next = (t - into_hour + HOUR_MS).min(end);
        let seg = (t, next);
        t = next;
        Some(seg)
    })
}

/// Timestamps where attention moved from one app/site to another.
fn switch_times(rows: &[Row]) -> Vec<i64> {
    let mut out = Vec::new();
    let mut prev: Option<&str> = None;
    for r in rows {
        if r.idle {
            // Coming back after a real break is not a context switch.
            if r.dur() >= 5 * MIN_MS {
                prev = None;
            }
            continue;
        }
        if r.dur() < MIN_VISIT_MS {
            continue;
        }
        if prev.is_some_and(|p| p != r.label()) {
            out.push(r.start);
        }
        prev = Some(r.label());
    }
    out
}

struct FocusBlock {
    productive_ms: i64,
}

/// Stretches of productive work that tolerate brief interruptions.
fn focus_blocks(rows: &[Row]) -> Vec<FocusBlock> {
    let mut out = Vec::new();
    let mut cur: Option<FocusBlock> = None;
    let mut gap = 0;
    let mut gap_distracting = 0;
    let mut prev_end: Option<i64> = None;

    for r in rows {
        // Missing time (paused, asleep) counts as a gap too.
        if let Some(prev_end) = prev_end {
            gap += (r.start - prev_end).max(0);
        }
        prev_end = Some(r.end);

        if !r.idle && r.category == Category::Productive {
            if gap > FOCUS_MAX_GAP_MS || gap_distracting > FOCUS_MAX_DISTRACTION_MS {
                out.extend(cur.take());
            }
            cur.get_or_insert(FocusBlock { productive_ms: 0 }).productive_ms += r.dur();
            gap = 0;
            gap_distracting = 0;
        } else {
            gap += r.dur();
            if !r.idle && r.category == Category::Distracting {
                gap_distracting += r.dur();
            }
        }
    }
    out.extend(cur);
    out
}

#[derive(Default, Clone)]
struct Acc {
    active: i64,
    productive: i64,
    neutral: i64,
    distracting: i64,
    switches: u32,
    distracted_by: HashMap<String, i64>,
}

impl Acc {
    fn add(&mut self, category: Category, ms: i64) {
        self.active += ms;
        match category {
            Category::Productive => self.productive += ms,
            Category::Neutral => self.neutral += ms,
            Category::Distracting => self.distracting += ms,
        }
    }

    fn merge(&mut self, other: &Acc) {
        self.active += other.active;
        self.productive += other.productive;
        self.neutral += other.neutral;
        self.distracting += other.distracting;
        self.switches += other.switches;
        for (k, v) in &other.distracted_by {
            *self.distracted_by.entry(k.clone()).or_default() += v;
        }
    }

    fn productive_share(&self) -> f64 {
        share(self.productive, self.active)
    }

    fn distracting_share(&self) -> f64 {
        share(self.distracting, self.active)
    }

    fn switches_per_hour(&self) -> f64 {
        if self.active == 0 { 0.0 } else { self.switches as f64 / (self.active as f64 / HOUR_MS as f64) }
    }

    fn top_distraction(&self) -> Option<&str> {
        self.distracted_by.iter().max_by_key(|(_, ms)| **ms).map(|(k, _)| k.as_str())
    }
}

fn share(part: i64, whole: i64) -> f64 {
    if whole == 0 { 0.0 } else { part as f64 / whole as f64 }
}

// ---------- today ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// `app:<exe>` or `site:<name>`, for the app's icon.
    key: String,
    label: String,
    category: Category,
    ms: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    start: i64,
    end: i64,
    category: Category,
    label: String,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    active_ms: i64,
    productive_ms: i64,
    neutral_ms: i64,
    distracting_ms: i64,
    switches: usize,
    longest_focus_ms: i64,
    deep_blocks: usize,
    top_apps: Vec<Usage>,
    top_sites: Vec<Usage>,
    timeline: Vec<Segment>,
}

/// A day at a glance, for the command palette: totals, the busiest apps and the timeline.
pub struct Glance {
    pub active_ms: i64,
    pub productive_ms: i64,
    pub top_apps: Vec<(String, i64)>,
    pub segments: Vec<(i64, i64, Category)>,
}

pub fn glance(rows: &[Row]) -> Glance {
    let mut acc = Acc::default();
    for r in rows.iter().filter(|r| !r.idle) {
        acc.add(r.category, r.dur());
    }
    let (apps, _) = top_apps_and_sites(rows, 8);
    Glance {
        active_ms: acc.active,
        productive_ms: acc.productive,
        top_apps: apps.into_iter().map(|u| (u.label, u.ms)).collect(),
        segments: timeline(rows).into_iter().map(|s| (s.start, s.end, s.category)).collect(),
    }
}

/// The apps and sites that took the most time in `rows`, `n` of each.
fn top_apps_and_sites(rows: &[Row], n: usize) -> (Vec<Usage>, Vec<Usage>) {
    // Apps group by name, since Windows Store apps all share one host program.
    let mut apps: HashMap<&str, (String, Category, i64)> = HashMap::new();
    let mut sites: HashMap<&str, (String, Category, i64)> = HashMap::new();
    for r in rows.iter().filter(|r| !r.idle) {
        apps.entry(&r.app).or_insert_with(|| (format!("app:{}", r.exe), r.app_category, 0)).2 += r.dur();
        if let Some(site) = &r.site {
            sites.entry(site).or_insert_with(|| (format!("site:{site}"), r.category, 0)).2 += r.dur();
        }
    }
    (top_usage(apps, n), top_usage(sites, n))
}

pub fn day_summary(rows: &[Row]) -> DaySummary {
    let mut acc = Acc::default();
    for r in rows.iter().filter(|r| !r.idle) {
        acc.add(r.category, r.dur());
    }
    let (top_apps, top_sites) = top_apps_and_sites(rows, 8);

    let blocks = focus_blocks(rows);
    DaySummary {
        active_ms: acc.active,
        productive_ms: acc.productive,
        neutral_ms: acc.neutral,
        distracting_ms: acc.distracting,
        switches: switch_times(rows).len(),
        longest_focus_ms: blocks.iter().map(|b| b.productive_ms).max().unwrap_or(0),
        deep_blocks: blocks.iter().filter(|b| b.productive_ms >= DEEP_BLOCK_MS).count(),
        top_apps,
        top_sites,
        timeline: timeline(rows),
    }
}

fn top_usage(map: HashMap<&str, (String, Category, i64)>, n: usize) -> Vec<Usage> {
    let mut list: Vec<Usage> = map
        .into_iter()
        .map(|(label, (key, category, ms))| Usage { key, label: label.to_string(), category, ms })
        .collect();
    list.sort_by_key(|u| std::cmp::Reverse(u.ms));
    list.truncate(n);
    list
}

/// Consecutive same-category activity merged into segments, labelled by the biggest contributor.
fn timeline(rows: &[Row]) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut weights: HashMap<String, i64> = HashMap::new();

    let flush = |seg: &mut Segment, weights: &mut HashMap<String, i64>| {
        if let Some((label, _)) = weights.drain().max_by_key(|(_, ms)| *ms) {
            seg.label = label;
        }
    };

    for r in rows.iter().filter(|r| !r.idle) {
        let extends = out.last().is_some_and(|s| s.category == r.category && r.start - s.end <= MIN_MS);
        if extends {
            out.last_mut().unwrap().end = r.end;
        } else {
            if let Some(last) = out.last_mut() {
                flush(last, &mut weights);
            }
            out.push(Segment { start: r.start, end: r.end, category: r.category, label: String::new() });
        }
        *weights.entry(r.label().to_string()).or_default() += r.dur();
    }
    if let Some(last) = out.last_mut() {
        flush(last, &mut weights);
    }
    out
}

// ---------- patterns ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HourStat {
    hour: u32,
    active_ms: i64,
    productive_ms: i64,
    neutral_ms: i64,
    distracting_ms: i64,
    switches: u32,
}

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    active_ms: i64,
    productive_ms: i64,
    distracting_ms: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InsightKind {
    Warning,
    Positive,
    Info,
}

#[derive(Serialize)]
pub struct Insight {
    kind: InsightKind,
    title: String,
    detail: String,
    /// The headline number ("88%", "3.2×") and what it measures, for the dashboard's metric column.
    metric: String,
    unit: String,
}

impl Insight {
    pub fn new(kind: InsightKind, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Insight { kind, title: title.into(), detail: detail.into(), metric: String::new(), unit: String::new() }
    }

    pub fn with_metric(mut self, metric: impl Into<String>, unit: impl Into<String>) -> Self {
        self.metric = metric.into();
        self.unit = unit.into();
        self
    }
}

/// Averages per day with meaningful activity, so quiet days don't drag them down.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PatternSummary {
    avg_active_ms: i64,
    avg_productive_ms: i64,
    avg_distracting_ms: i64,
    focus_sessions: usize,
    avg_focus_ms: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Patterns {
    /// Days in the range with meaningful activity; the hourly totals cover these.
    days_with_data: usize,
    summary: PatternSummary,
    hourly: Vec<HourStat>,
    /// Monday-first, 7 x 24.
    heatmap: Vec<Vec<Cell>>,
    top_apps: Vec<Usage>,
    top_sites: Vec<Usage>,
    insights: Vec<Insight>,
}

impl Patterns {
    /// Folds in focus sessions from the same range, which live outside the activity rows.
    pub fn with_focus(mut self, sessions: usize, focused_ms: i64, insights: Vec<Insight>) -> Self {
        self.summary.focus_sessions = sessions;
        self.summary.avg_focus_ms = focused_ms / self.days_with_data.max(1) as i64;
        // Focus insights only join a page that already has real ones, not the "still learning" note.
        if self.days_with_data >= MIN_DAYS_FOR_INSIGHTS {
            self.insights.extend(insights);
        }
        self
    }
}

/// Per-hour totals, the weekday x hour grid, and the days with meaningful activity.
fn accumulate(rows: &[Row]) -> (Vec<Acc>, Vec<Vec<Cell>>, Vec<NaiveDate>) {
    let mut hourly = vec![Acc::default(); 24];
    let mut grid = vec![vec![Cell::default(); 24]; 7];
    let mut day_active: HashMap<NaiveDate, i64> = HashMap::new();

    for r in rows.iter().filter(|r| !r.idle) {
        for (t0, t1) in split_by_hour(r.start, r.end) {
            let dt = local(t0);
            let (h, wd, ms) = (dt.hour() as usize, dt.weekday().num_days_from_monday() as usize, t1 - t0);
            hourly[h].add(r.category, ms);
            if r.category == Category::Distracting {
                *hourly[h].distracted_by.entry(r.label().to_string()).or_default() += ms;
            }
            let cell = &mut grid[wd][h];
            cell.active_ms += ms;
            match r.category {
                Category::Productive => cell.productive_ms += ms,
                Category::Distracting => cell.distracting_ms += ms,
                Category::Neutral => {}
            }
            *day_active.entry(dt.date_naive()).or_default() += ms;
        }
    }
    for t in switch_times(rows) {
        hourly[local(t).hour() as usize].switches += 1;
    }

    let active_days: Vec<NaiveDate> =
        day_active.iter().filter(|(_, ms)| **ms >= MIN_DAY_ACTIVE_MS).map(|(d, _)| *d).collect();
    (hourly, grid, active_days)
}

pub fn patterns(rows: &[Row]) -> Patterns {
    let (hourly, grid, active_days) = accumulate(rows);
    let insights = insights(rows, &hourly, &grid, &active_days);
    let total = sum_hours(&hourly, 0..24);
    let days = active_days.len().max(1) as i64;
    let (top_apps, top_sites) = top_apps_and_sites(rows, 6);

    Patterns {
        days_with_data: active_days.len(),
        summary: PatternSummary {
            avg_active_ms: total.active / days,
            avg_productive_ms: total.productive / days,
            avg_distracting_ms: total.distracting / days,
            ..PatternSummary::default()
        },
        top_apps,
        top_sites,
        hourly: hourly
            .iter()
            .enumerate()
            .map(|(h, a)| HourStat {
                hour: h as u32,
                active_ms: a.active,
                productive_ms: a.productive,
                neutral_ms: a.neutral,
                distracting_ms: a.distracting,
                switches: a.switches,
            })
            .collect(),
        heatmap: grid,
        insights,
    }
}

// ---------- insights ----------

struct TimeWindow {
    phrase: &'static str,
    short: &'static str,
    hours: &'static [usize],
}

const WINDOWS: &[TimeWindow] = &[
    TimeWindow { phrase: "in the early morning (5–9am)", short: "5–9am", hours: &[5, 6, 7, 8] },
    TimeWindow { phrase: "in the morning (9am–12pm)", short: "9am–12pm", hours: &[9, 10, 11] },
    TimeWindow { phrase: "in the afternoon (12–5pm)", short: "12–5pm", hours: &[12, 13, 14, 15, 16] },
    TimeWindow { phrase: "in the evening (5–10pm)", short: "5–10pm", hours: &[17, 18, 19, 20, 21] },
    TimeWindow { phrase: "after 10pm", short: "after 10pm", hours: &[22, 23, 0, 1, 2, 3, 4] },
];

/// The part of the day where distraction share stands out most from the overall rate, if any does.
fn worst_window(hourly: &[Acc], total: &Acc) -> Option<(&'static TimeWindow, Acc)> {
    let overall = total.distracting_share();
    WINDOWS
        .iter()
        .map(|w| (w, sum_hours(hourly, w.hours.iter().copied())))
        .filter(|(_, acc)| acc.active >= (45 * MIN_MS).max(total.active / 20))
        .max_by(|a, b| a.1.distracting_share().total_cmp(&b.1.distracting_share()))
        .filter(|(_, acc)| {
            let s = acc.distracting_share();
            s >= overall * 1.3 && s - overall >= 0.08
        })
}

/// The hours a user reliably loses focus in, used for live nudges.
#[derive(Clone)]
pub struct LowFocusWindow {
    pub hours: &'static [usize],
    /// "after 10pm", "in the evening (5–10pm)".
    pub phrase: &'static str,
    pub share: f64,
    pub overall: f64,
}

pub fn low_focus_window(rows: &[Row]) -> Option<LowFocusWindow> {
    let (hourly, _, active_days) = accumulate(rows);
    if active_days.len() < MIN_DAYS_FOR_INSIGHTS {
        return None;
    }
    let total = sum_hours(&hourly, 0..24);
    let (w, acc) = worst_window(&hourly, &total)?;
    Some(LowFocusWindow { hours: w.hours, phrase: w.phrase, share: acc.distracting_share(), overall: total.distracting_share() })
}

const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

fn sum_hours(hourly: &[Acc], hours: impl IntoIterator<Item = usize>) -> Acc {
    let mut acc = Acc::default();
    for h in hours {
        acc.merge(&hourly[h % 24]);
    }
    acc
}

fn insights(rows: &[Row], hourly: &[Acc], grid: &[Vec<Cell>], active_days: &[NaiveDate]) -> Vec<Insight> {
    let days = active_days.len();
    if days < MIN_DAYS_FOR_INSIGHTS {
        return vec![Insight::new(
            InsightKind::Info,
            "Still learning your patterns",
            format!(
                "Kairo needs about {MIN_DAYS_FOR_INSIGHTS} days of activity before it can spot patterns, and it has {days} so far. Keep it running in the background."
            ),
        )
        .with_metric(format!("{days}/{MIN_DAYS_FOR_INSIGHTS}"), "days collected")];
    }

    let total = sum_hours(hourly, 0..24);
    let mut out = Vec::new();
    // Two-hour windows are compared only when each hour has enough data to mean something.
    let enough = |h: usize| hourly[h % 24].active >= 30 * MIN_MS;

    // When does distraction peak?
    let overall = total.distracting_share();
    if let Some((w, acc)) = worst_window(hourly, &total) {
        let s = acc.distracting_share();
        let culprit = acc.top_distraction().map(|c| format!(" The biggest pull is {c}.")).unwrap_or_default();
        out.push(Insight::new(
            InsightKind::Warning,
            format!("You lose focus most {}", w.phrase),
            format!(
                "{}% of your screen time {} goes to distracting apps and sites, compared with {}% overall.{culprit}",
                pct(s),
                w.phrase,
                pct(overall)
            ),
        )
        .with_metric(format!("{}%", pct(s)), format!("distracted {}", w.short)));
    }

    // When is focus best?
    let best = (0..24)
        .filter(|&h| enough(h) && enough(h + 1))
        .map(|h| (h, sum_hours(hourly, [h, h + 1])))
        .max_by(|a, b| a.1.productive_share().total_cmp(&b.1.productive_share()));
    if let Some((h, acc)) = best {
        let (s, o) = (acc.productive_share(), total.productive_share());
        if s >= 0.4 && s >= o + 0.1 {
            out.push(Insight::new(
                InsightKind::Positive,
                format!("Your sharpest hours are {}", hour_range(h, h + 2)),
                format!(
                    "{}% of your time between {} is productive work, compared with {}% overall. It's the best slot for deep work.",
                    pct(s),
                    hour_range(h, h + 2),
                    pct(o)
                ),
            )
            .with_metric(format!("{}%", pct(s)), format!("productive, {}", hour_range(h, h + 2))));
        }
    }

    // When does attention fragment?
    let busiest = (0..24)
        .filter(|&h| enough(h) && enough(h + 1))
        .map(|h| (h, sum_hours(hourly, [h, h + 1])))
        .max_by(|a, b| a.1.switches_per_hour().total_cmp(&b.1.switches_per_hour()));
    if let Some((h, acc)) = busiest {
        let (r, o) = (acc.switches_per_hour(), total.switches_per_hour());
        if r >= 15.0 && r >= o * 1.5 {
            out.push(Insight::new(
                InsightKind::Info,
                format!("You switch apps most between {}", hour_range(h, h + 2)),
                format!(
                    "About {} switches per active hour, versus {} on average. Constant switching usually means fragmented attention, so try batching messages and email outside this window.",
                    r.round(),
                    o.round()
                ),
            )
            .with_metric(format!("{}", r.round()), "switches an hour"));
        }
    }

    // What usually comes right before a distraction?
    if let Some(insight) = distraction_trigger(rows) {
        out.push(insight);
    }

    // Which weekday is best?
    let mut weekday_days = [0usize; 7];
    for d in active_days {
        weekday_days[d.weekday().num_days_from_monday() as usize] += 1;
    }
    let by_day: Vec<(usize, f64)> = grid
        .iter()
        .enumerate()
        .filter(|(wd, _)| weekday_days[*wd] >= 2)
        .filter_map(|(wd, cells)| {
            let active: i64 = cells.iter().map(|c| c.active_ms).sum();
            let productive: i64 = cells.iter().map(|c| c.productive_ms).sum();
            (active >= 2 * HOUR_MS).then(|| (wd, share(productive, active)))
        })
        .collect();
    if by_day.len() >= 3 {
        let best = by_day.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
        let worst = by_day.iter().min_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
        if best.1 - worst.1 >= 0.15 {
            out.push(Insight::new(
                InsightKind::Positive,
                format!("{}s are your most focused day", WEEKDAYS[best.0]),
                format!(
                    "{}% of your time is productive on {}s, versus {}% on {}s.",
                    pct(best.1),
                    WEEKDAYS[best.0],
                    pct(worst.1),
                    WEEKDAYS[worst.0]
                ),
            )
            .with_metric(format!("{}%", pct(best.1)), format!("productive on {}s", WEEKDAYS[best.0])));
        }
    }

    // How much deep work is happening?
    let blocks = focus_blocks(rows);
    let deep: Vec<&FocusBlock> = blocks.iter().filter(|b| b.productive_ms >= DEEP_BLOCK_MS).collect();
    if deep.is_empty() {
        out.push(Insight::new(
            InsightKind::Warning,
            "No deep-work blocks yet",
            "None of your productive stretches lasted 25 minutes without a real break. Try one distraction-free block in your best hours.",
        )
        .with_metric("0", "deep blocks"));
    } else {
        let longest = deep.iter().map(|b| b.productive_ms).max().unwrap_or(0);
        out.push(Insight::new(
            InsightKind::Info,
            format!("You average {:.1} deep-work blocks a day", deep.len() as f64 / days as f64),
            format!(
                "A deep-work block is 25+ minutes of productive work without a real break. Your longest in this period was {}.",
                fmt_duration(longest)
            ),
        )
        .with_metric(format!("{:.1}", deep.len() as f64 / days as f64), "deep blocks a day"));
    }

    out
}

/// Finds the app or site that most often leads straight into a distraction.
/// Compares rates, not counts: the app you use most will always precede the most distractions.
fn distraction_trigger(rows: &[Row]) -> Option<Insight> {
    // label -> (times you left it, times you left it for something distracting)
    let mut exits: HashMap<&str, (u32, u32)> = HashMap::new();
    let mut prev: Option<&Row> = None;
    for r in rows.iter().filter(|r| !r.idle && r.dur() >= MIN_VISIT_MS) {
        if let Some(p) = prev {
            let is_exit = p.category != Category::Distracting && p.label() != r.label() && r.start - p.end < MIN_MS;
            if is_exit {
                let e = exits.entry(p.label()).or_default();
                e.0 += 1;
                if r.category == Category::Distracting && r.dur() >= MIN_MS {
                    e.1 += 1;
                }
            }
        }
        prev = Some(r);
    }

    let (all_exits, all_hits) = exits.values().fold((0, 0), |(n, k), (en, ek)| (n + en, k + ek));
    if all_hits < 8 {
        return None;
    }
    let baseline = all_hits as f64 / all_exits as f64;
    let (label, (n, k)) = exits
        .into_iter()
        .filter(|(_, (n, k))| *n >= 10 && *k >= 4)
        .max_by(|a, b| (a.1 .1 as f64 / a.1 .0 as f64).total_cmp(&(b.1 .1 as f64 / b.1 .0 as f64)))?;
    let rate = k as f64 / n as f64;
    if rate < baseline * 1.5 || rate < 0.2 {
        return None;
    }
    Some(Insight::new(
        InsightKind::Warning,
        format!("Distractions often start right after {label}"),
        format!(
            "{}% of the time you leave {label}, you go straight to something distracting, {:.1}× your usual rate. Checking it at planned times could break the chain.",
            pct(rate),
            rate / baseline
        ),
    )
    .with_metric(format!("{:.1}×", rate / baseline), "your usual rate"))
}

// ---------- formatting ----------

pub fn pct(x: f64) -> i64 {
    (x * 100.0).round() as i64
}

fn fmt_duration(ms: i64) -> String {
    let mins = ms / MIN_MS;
    if mins >= 60 { format!("{}h {}m", mins / 60, mins % 60) } else { format!("{mins}m") }
}

fn hour_parts(h: usize) -> (usize, &'static str) {
    let h = h % 24;
    let n = if h.is_multiple_of(12) { 12 } else { h % 12 };
    (n, if h < 12 { "am" } else { "pm" })
}

/// "9–11am", "11am–1pm".
fn hour_range(from: usize, to: usize) -> String {
    let ((a, sa), (b, sb)) = (hour_parts(from), hour_parts(to));
    if sa == sb { format!("{a}–{b}{sb}") } else { format!("{a}{sa}–{b}{sb}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(label: &str, category: Category, start_min: i64, end_min: i64) -> Row {
        Row {
            app: label.into(),
            exe: label.into(),
            site: None,
            start: start_min * MIN_MS,
            end: end_min * MIN_MS,
            idle: false,
            category,
            app_category: category,
        }
    }

    #[test]
    fn hour_ranges_read_naturally() {
        assert_eq!(hour_range(9, 11), "9–11am");
        assert_eq!(hour_range(11, 13), "11am–1pm");
        assert_eq!(hour_range(22, 24), "10pm–12am");
    }

    #[test]
    fn focus_block_survives_short_interruption_but_not_long_distraction() {
        use Category::*;
        let rows = vec![
            row("code", Productive, 0, 20),
            row("slack", Neutral, 20, 22),
            row("code", Productive, 22, 40),
            row("youtube", Distracting, 40, 50),
            row("code", Productive, 50, 60),
        ];
        let blocks = focus_blocks(&rows);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].productive_ms, 38 * MIN_MS);
        assert_eq!(blocks[1].productive_ms, 10 * MIN_MS);
    }

    #[test]
    fn switches_ignore_pass_through_windows() {
        use Category::*;
        let mut rows = vec![row("code", Productive, 0, 10), row("slack", Neutral, 10, 12)];
        rows.push(Row { start: 12 * MIN_MS, end: 12 * MIN_MS + 500, ..row("explorer", Neutral, 0, 0) });
        rows.push(row("code", Productive, 12, 20));
        assert_eq!(switch_times(&rows).len(), 2);
    }

    #[test]
    fn split_by_hour_covers_whole_range() {
        let start = 1_700_000_000_000;
        let end = start + 3 * HOUR_MS + 17 * MIN_MS;
        let parts: Vec<_> = split_by_hour(start, end).collect();
        assert_eq!(parts.first().unwrap().0, start);
        assert_eq!(parts.last().unwrap().1, end);
        assert!(parts.windows(2).all(|w| w[0].1 == w[1].0));
    }
}
