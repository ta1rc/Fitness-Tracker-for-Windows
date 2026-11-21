mod coach;
mod db;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use db::now_ms;

struct Db {
    conn: Mutex<Connection>,
    path: PathBuf,
}

type R<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

impl Db {
    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>) -> R<T> {
        let mut conn = self.conn.lock().map_err(err)?;
        f(&mut conn).map_err(err)
    }
}

// ———————————————————————————————————————————————————————————— settings

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Settings {
    name: String,
    units: String, // "metric" | "imperial"
    kcal: f64,
    protein: f64,
    carbs: f64,
    fat: f64,
    fiber: f64,
    water_ml: f64,
    steps: i64,
    sleep_h: f64,
    workouts_per_week: i64,
    goal_weight_kg: Option<f64>,
    // Profile, used only by the energy calculator.
    sex: String, // "male" | "female"
    birth_year: Option<i64>,
    height_cm: Option<f64>,
    activity: f64,
    pace_kg_week: f64,
    onboarded: bool,
    /// Ambient background: "auto" or a scene name.
    scene: String,
    /// Let the background respond to how much of the day is done.
    reactive: bool,
    /// For meal suggestions: any | pescatarian | eggetarian | vegetarian | vegan.
    diet: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            name: String::new(),
            units: "metric".into(),
            kcal: 2200.0,
            protein: 150.0,
            carbs: 220.0,
            fat: 70.0,
            fiber: 30.0,
            water_ml: 2500.0,
            steps: 8000,
            sleep_h: 8.0,
            workouts_per_week: 4,
            goal_weight_kg: None,
            sex: "male".into(),
            birth_year: None,
            height_cm: None,
            activity: 1.45,
            pace_kg_week: -0.25,
            onboarded: false,
            scene: "auto".into(),
            reactive: true,
            diet: "any".into(),
        }
    }
}

#[tauri::command]
fn get_settings(db: State<Db>) -> R<Settings> {
    db.with(|c| {
        let raw: Option<String> = c
            .query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get(0))
            .optional()?;
        Ok(raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
    })
}

#[tauri::command]
fn save_settings(db: State<Db>, settings: Settings) -> R<Settings> {
    let json = serde_json::to_string(&settings).map_err(err)?;
    db.with(|c| {
        c.execute(
            "INSERT INTO settings (key, value) VALUES ('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )
    })?;
    Ok(settings)
}

// ———————————————————————————————————————————————————————————— foods

#[derive(Serialize)]
struct Food {
    id: i64,
    name: String,
    serving: String,
    kcal: f64,
    protein: f64,
    carbs: f64,
    fat: f64,
    fiber: f64,
    favorite: bool,
    use_count: i64,
    last_used_at: Option<i64>,
}

const FOOD_COLS: &str =
    "id, name, serving, kcal, protein, carbs, fat, fiber, favorite, use_count, last_used_at";

fn food_row(r: &Row) -> rusqlite::Result<Food> {
    Ok(Food {
        id: r.get(0)?,
        name: r.get(1)?,
        serving: r.get(2)?,
        kcal: r.get(3)?,
        protein: r.get(4)?,
        carbs: r.get(5)?,
        fat: r.get(6)?,
        fiber: r.get(7)?,
        favorite: r.get::<_, i64>(8)? != 0,
        use_count: r.get(9)?,
        last_used_at: r.get(10)?,
    })
}

#[derive(Deserialize)]
struct FoodInput {
    id: Option<i64>,
    name: String,
    serving: String,
    kcal: f64,
    protein: f64,
    carbs: f64,
    fat: f64,
    #[serde(default)]
    fiber: f64,
}

/// Empty query: favourites, then most recently and most often used.
/// Otherwise every word must appear somewhere in the name.
#[tauri::command]
fn search_foods(db: State<Db>, query: String, limit: Option<i64>) -> R<Vec<Food>> {
    let limit = limit.unwrap_or(40);
    let words: Vec<String> = query
        .split_whitespace()
        .map(|w| format!("%{}%", w.replace('%', "").replace('_', "")))
        .collect();
    db.with(|c| {
        let mut sql = format!("SELECT {FOOD_COLS} FROM foods WHERE archived = 0");
        for i in 0..words.len() {
            sql.push_str(&format!(" AND name LIKE ?{}", i + 1));
        }
        if let Some(first) = query.split_whitespace().next() {
            // Names that start with the first word rank above mid-word matches.
            let first = first.replace('\'', "''");
            sql.push_str(&format!(
                " ORDER BY (name LIKE '{first}%') DESC, favorite DESC, use_count DESC, name COLLATE NOCASE"
            ));
        } else {
            sql.push_str(" ORDER BY favorite DESC, COALESCE(last_used_at, 0) DESC, use_count DESC, name COLLATE NOCASE");
        }
        sql.push_str(&format!(" LIMIT {limit}"));
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(words.iter()), food_row)?;
        rows.collect()
    })
}

#[tauri::command]
fn save_food(db: State<Db>, food: FoodInput) -> R<Food> {
    db.with(|c| {
        let id = match food.id {
            Some(id) => {
                c.execute(
                    "UPDATE foods SET name=?1, serving=?2, kcal=?3, protein=?4, carbs=?5, fat=?6, fiber=?7 WHERE id=?8",
                    params![food.name.trim(), food.serving.trim(), food.kcal, food.protein, food.carbs, food.fat, food.fiber, id],
                )?;
                id
            }
            None => {
                c.execute(
                    "INSERT INTO foods (name, serving, kcal, protein, carbs, fat, fiber, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![food.name.trim(), food.serving.trim(), food.kcal, food.protein, food.carbs, food.fat, food.fiber, now_ms()],
                )?;
                c.last_insert_rowid()
            }
        };
        c.query_row(&format!("SELECT {FOOD_COLS} FROM foods WHERE id = ?1"), [id], food_row)
    })
}

/// Archived rather than deleted, so diary entries keep their link.
#[tauri::command]
fn delete_food(db: State<Db>, id: i64) -> R<()> {
    db.with(|c| c.execute("UPDATE foods SET archived = 1 WHERE id = ?1", [id]).map(|_| ()))
}

#[tauri::command]
fn toggle_favorite(db: State<Db>, id: i64) -> R<bool> {
    db.with(|c| {
        c.execute("UPDATE foods SET favorite = 1 - favorite WHERE id = ?1", [id])?;
        c.query_row("SELECT favorite FROM foods WHERE id = ?1", [id], |r| r.get::<_, i64>(0))
            .map(|v| v != 0)
    })
}

// ———————————————————————————————————————————————————————————— diary entries

#[derive(Serialize)]
struct Entry {
    id: i64,
    day: String,
    meal: String,
    food_id: Option<i64>,
    name: String,
    serving: String,
    qty: f64,
    kcal: f64,
    protein: f64,
    carbs: f64,
    fat: f64,
    fiber: f64,
    created_at: i64,
}

const ENTRY_COLS: &str =
    "id, day, meal, food_id, name, serving, qty, kcal, protein, carbs, fat, fiber, created_at";

fn entry_row(r: &Row) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        day: r.get(1)?,
        meal: r.get(2)?,
        food_id: r.get(3)?,
        name: r.get(4)?,
        serving: r.get(5)?,
        qty: r.get(6)?,
        kcal: r.get(7)?,
        protein: r.get(8)?,
        carbs: r.get(9)?,
        fat: r.get(10)?,
        fiber: r.get(11)?,
        created_at: r.get(12)?,
    })
}

#[derive(Deserialize)]
struct EntryInput {
    day: String,
    meal: String,
    food_id: Option<i64>,
    name: String,
    serving: String,
    qty: f64,
    kcal: f64,
    protein: f64,
    carbs: f64,
    fat: f64,
    #[serde(default)]
    fiber: f64,
    /// For a quick-add with no food: also keep it in the library.
    #[serde(default)]
    save_food: bool,
}

#[tauri::command]
fn add_entry(db: State<Db>, entry: EntryInput) -> R<Entry> {
    db.with(|c| {
        let tx = c.transaction()?;
        let now = now_ms();
        let mut food_id = entry.food_id;
        if food_id.is_none() && entry.save_food {
            tx.execute(
                "INSERT INTO foods (name, serving, kcal, protein, carbs, fat, fiber, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![entry.name.trim(), entry.serving.trim(), entry.kcal, entry.protein, entry.carbs, entry.fat, entry.fiber, now],
            )?;
            food_id = Some(tx.last_insert_rowid());
        }
        if let Some(fid) = food_id {
            tx.execute(
                "UPDATE foods SET use_count = use_count + 1, last_used_at = ?1 WHERE id = ?2",
                params![now, fid],
            )?;
        }
        tx.execute(
            "INSERT INTO entries (day, meal, food_id, name, serving, qty, kcal, protein, carbs, fat, fiber, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![entry.day, entry.meal, food_id, entry.name.trim(), entry.serving.trim(), entry.qty,
                    entry.kcal, entry.protein, entry.carbs, entry.fat, entry.fiber, now],
        )?;
        let id = tx.last_insert_rowid();
        let e = tx.query_row(&format!("SELECT {ENTRY_COLS} FROM entries WHERE id = ?1"), [id], entry_row)?;
        tx.commit()?;
        Ok(e)
    })
}

#[tauri::command]
fn update_entry(db: State<Db>, id: i64, qty: f64, meal: String) -> R<()> {
    db.with(|c| {
        c.execute("UPDATE entries SET qty = ?1, meal = ?2 WHERE id = ?3", params![qty, meal, id])
            .map(|_| ())
    })
}

#[derive(Deserialize)]
struct EntryEdit {
    name: String,
    serving: String,
    meal: String,
    qty: f64,
    /// Per-serving values; the entry's totals are these × qty.
    kcal: f64,
    protein: f64,
    carbs: f64,
    fat: f64,
    #[serde(default)]
    fiber: f64,
}

/// Rewrite a logged entry. With `update_food`, the library food it came from
/// takes the same per-serving values (earlier entries keep theirs).
#[tauri::command]
fn edit_entry(db: State<Db>, id: i64, edit: EntryEdit, update_food: bool) -> R<Entry> {
    db.with(|c| apply_entry_edit(c, id, &edit, update_food))
}

fn apply_entry_edit(c: &mut Connection, id: i64, e: &EntryEdit, update_food: bool) -> rusqlite::Result<Entry> {
    let tx = c.transaction()?;
    tx.execute(
        "UPDATE entries SET name=?1, serving=?2, meal=?3, qty=?4, kcal=?5, protein=?6, carbs=?7, fat=?8, fiber=?9 WHERE id=?10",
        params![e.name.trim(), e.serving.trim(), e.meal, e.qty, e.kcal, e.protein, e.carbs, e.fat, e.fiber, id],
    )?;
    if update_food {
        tx.execute(
            "UPDATE foods SET name=?1, serving=?2, kcal=?3, protein=?4, carbs=?5, fat=?6, fiber=?7
             WHERE id = (SELECT food_id FROM entries WHERE id = ?8)",
            params![e.name.trim(), e.serving.trim(), e.kcal, e.protein, e.carbs, e.fat, e.fiber, id],
        )?;
    }
    let out = tx.query_row(&format!("SELECT {ENTRY_COLS} FROM entries WHERE id = ?1"), [id], entry_row)?;
    tx.commit()?;
    Ok(out)
}

#[tauri::command]
fn delete_entry(db: State<Db>, id: i64) -> R<()> {
    db.with(|c| c.execute("DELETE FROM entries WHERE id = ?1", [id]).map(|_| ()))
}

/// Copy one meal's entries from another day. Returns how many were copied.
#[tauri::command]
fn copy_meal(db: State<Db>, from_day: String, to_day: String, meal: String) -> R<usize> {
    db.with(|c| {
        c.execute(
            "INSERT INTO entries (day, meal, food_id, name, serving, qty, kcal, protein, carbs, fat, fiber, created_at)
             SELECT ?1, meal, food_id, name, serving, qty, kcal, protein, carbs, fat, fiber, ?2
             FROM entries WHERE day = ?3 AND meal = ?4 ORDER BY id",
            params![to_day, now_ms(), from_day, meal],
        )
    })
}

// ———————————————————————————————————————————————————————————— workouts

#[derive(Serialize, Deserialize, Clone)]
struct WorkSet {
    exercise: String,
    reps: Option<i64>,
    weight_kg: Option<f64>,
}

#[derive(Serialize)]
struct Workout {
    id: i64,
    day: String,
    title: String,
    kind: String,
    duration_min: f64,
    distance_km: Option<f64>,
    kcal: Option<f64>,
    effort: Option<i64>,
    notes: String,
    created_at: i64,
    sets: Vec<WorkSet>,
}

#[derive(Deserialize)]
struct WorkoutInput {
    id: Option<i64>,
    day: String,
    title: String,
    kind: String,
    duration_min: f64,
    distance_km: Option<f64>,
    kcal: Option<f64>,
    effort: Option<i64>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    sets: Vec<WorkSet>,
}

const WORKOUT_COLS: &str =
    "id, day, title, kind, duration_min, distance_km, kcal, effort, notes, created_at";

fn load_workouts(c: &Connection, where_sql: &str, args: &[&dyn rusqlite::ToSql]) -> rusqlite::Result<Vec<Workout>> {
    let mut stmt = c.prepare(&format!("SELECT {WORKOUT_COLS} FROM workouts {where_sql}"))?;
    let mut list: Vec<Workout> = stmt
        .query_map(args, |r| {
            Ok(Workout {
                id: r.get(0)?,
                day: r.get(1)?,
                title: r.get(2)?,
                kind: r.get(3)?,
                duration_min: r.get(4)?,
                distance_km: r.get(5)?,
                kcal: r.get(6)?,
                effort: r.get(7)?,
                notes: r.get(8)?,
                created_at: r.get(9)?,
                sets: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut sets = c.prepare(
        "SELECT exercise, reps, weight_kg FROM sets WHERE workout_id = ?1 ORDER BY position",
    )?;
    for w in &mut list {
        w.sets = sets
            .query_map([w.id], |r| Ok(WorkSet { exercise: r.get(0)?, reps: r.get(1)?, weight_kg: r.get(2)? }))?
            .collect::<rusqlite::Result<_>>()?;
    }
    Ok(list)
}

#[tauri::command]
fn save_workout(db: State<Db>, workout: WorkoutInput) -> R<i64> {
    db.with(|c| {
        let tx = c.transaction()?;
        let w = &workout;
        let id = match w.id {
            Some(id) => {
                tx.execute(
                    "UPDATE workouts SET day=?1, title=?2, kind=?3, duration_min=?4, distance_km=?5, kcal=?6, effort=?7, notes=?8 WHERE id=?9",
                    params![w.day, w.title.trim(), w.kind, w.duration_min, w.distance_km, w.kcal, w.effort, w.notes, id],
                )?;
                tx.execute("DELETE FROM sets WHERE workout_id = ?1", [id])?;
                id
            }
            None => {
                tx.execute(
                    "INSERT INTO workouts (day, title, kind, duration_min, distance_km, kcal, effort, notes, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![w.day, w.title.trim(), w.kind, w.duration_min, w.distance_km, w.kcal, w.effort, w.notes, now_ms()],
                )?;
                tx.last_insert_rowid()
            }
        };
        {
            let mut ins = tx.prepare(
                "INSERT INTO sets (workout_id, position, exercise, reps, weight_kg) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (i, s) in w.sets.iter().enumerate() {
                if s.exercise.trim().is_empty() {
                    continue;
                }
                ins.execute(params![id, i as i64, s.exercise.trim(), s.reps, s.weight_kg])?;
            }
        }
        tx.commit()?;
        Ok(id)
    })
}

#[tauri::command]
fn delete_workout(db: State<Db>, id: i64) -> R<()> {
    db.with(|c| c.execute("DELETE FROM workouts WHERE id = ?1", [id]).map(|_| ()))
}

#[tauri::command]
fn list_workouts(db: State<Db>, from: String, to: String) -> R<Vec<Workout>> {
    db.with(|c| {
        load_workouts(
            c,
            "WHERE day BETWEEN ?1 AND ?2 ORDER BY day DESC, created_at DESC",
            &[&from, &to],
        )
    })
}

/// Distinct recent workouts by title, to start a new one from.
#[tauri::command]
fn recent_workouts(db: State<Db>, limit: Option<i64>) -> R<Vec<Workout>> {
    let limit = limit.unwrap_or(6);
    db.with(|c| {
        load_workouts(
            c,
            &format!(
                "WHERE id IN (SELECT MAX(id) FROM workouts GROUP BY title COLLATE NOCASE)
                 ORDER BY created_at DESC LIMIT {limit}"
            ),
            &[],
        )
    })
}

#[tauri::command]
fn exercise_names(db: State<Db>) -> R<Vec<String>> {
    db.with(|c| {
        let mut stmt = c.prepare(
            "SELECT exercise FROM sets GROUP BY exercise COLLATE NOCASE ORDER BY COUNT(*) DESC, exercise LIMIT 200",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    })
}

#[derive(Serialize)]
struct ExercisePoint {
    day: String,
    sets: i64,
    reps: i64,
    volume: f64,
    top_weight: f64,
    /// Epley estimate of the one-rep max, from the best set of the day.
    e1rm: f64,
}

#[derive(Serialize)]
struct ExerciseSummary {
    exercise: String,
    sessions: i64,
    last_day: String,
    top_weight: f64,
    e1rm: f64,
}

fn epley(weight: f64, reps: i64) -> f64 {
    if reps <= 1 {
        weight
    } else {
        weight * (1.0 + reps as f64 / 30.0)
    }
}

#[tauri::command]
fn exercise_history(db: State<Db>, exercise: String) -> R<Vec<ExercisePoint>> {
    db.with(|c| {
        let mut stmt = c.prepare(
            "SELECT w.day, s.reps, s.weight_kg FROM sets s JOIN workouts w ON w.id = s.workout_id
             WHERE s.exercise = ?1 COLLATE NOCASE ORDER BY w.day",
        )?;
        let mut by_day: BTreeMap<String, ExercisePoint> = BTreeMap::new();
        let rows = stmt.query_map([&exercise], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<f64>>(2)?))
        })?;
        for row in rows {
            let (day, reps, weight) = row?;
            let reps = reps.unwrap_or(0);
            let weight = weight.unwrap_or(0.0);
            let p = by_day.entry(day.clone()).or_insert(ExercisePoint {
                day,
                sets: 0,
                reps: 0,
                volume: 0.0,
                top_weight: 0.0,
                e1rm: 0.0,
            });
            p.sets += 1;
            p.reps += reps;
            p.volume += reps as f64 * weight;
            p.top_weight = p.top_weight.max(weight);
            p.e1rm = p.e1rm.max(epley(weight, reps));
        }
        Ok(by_day.into_values().collect())
    })
}

#[tauri::command]
fn exercise_summaries(db: State<Db>) -> R<Vec<ExerciseSummary>> {
    db.with(|c| {
        let mut stmt = c.prepare(
            "SELECT s.exercise, w.day, s.reps, s.weight_kg FROM sets s JOIN workouts w ON w.id = s.workout_id",
        )?;
        let mut map: BTreeMap<String, (String, std::collections::BTreeSet<String>, String, f64, f64)> =
            BTreeMap::new();
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<f64>>(3)?,
            ))
        })?;
        for row in rows {
            let (ex, day, reps, weight) = row?;
            let key = ex.to_lowercase();
            let e = map.entry(key).or_insert((ex.clone(), Default::default(), String::new(), 0.0, 0.0));
            e.1.insert(day.clone());
            if day > e.2 {
                e.2 = day;
            }
            let w = weight.unwrap_or(0.0);
            e.3 = e.3.max(w);
            e.4 = e.4.max(epley(w, reps.unwrap_or(0)));
        }
        let mut out: Vec<ExerciseSummary> = map
            .into_values()
            .map(|(exercise, days, last_day, top_weight, e1rm)| ExerciseSummary {
                exercise,
                sessions: days.len() as i64,
                last_day,
                top_weight,
                e1rm,
            })
            .collect();
        out.sort_by(|a, b| b.last_day.cmp(&a.last_day).then(b.sessions.cmp(&a.sessions)));
        Ok(out)
    })
}

// ———————————————————————————————————————————————————————————— day metrics

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct DayMetrics {
    water_ml: Option<f64>,
    steps: Option<i64>,
    sleep_h: Option<f64>,
    weight_kg: Option<f64>,
    body_fat: Option<f64>,
    energy: Option<i64>,
    note: Option<String>,
}

fn load_metrics(c: &Connection, day: &str) -> rusqlite::Result<DayMetrics> {
    Ok(c.query_row(
        "SELECT water_ml, steps, sleep_h, weight_kg, body_fat, energy, note FROM days WHERE day = ?1",
        [day],
        |r| {
            Ok(DayMetrics {
                water_ml: r.get(0)?,
                steps: r.get(1)?,
                sleep_h: r.get(2)?,
                weight_kg: r.get(3)?,
                body_fat: r.get(4)?,
                energy: r.get(5)?,
                note: r.get(6)?,
            })
        },
    )
    .optional()?
    .unwrap_or_default())
}

#[tauri::command]
fn set_metrics(db: State<Db>, day: String, metrics: DayMetrics) -> R<()> {
    let m = metrics;
    db.with(|c| {
        c.execute(
            "INSERT INTO days (day, water_ml, steps, sleep_h, weight_kg, body_fat, energy, note)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(day) DO UPDATE SET water_ml=excluded.water_ml, steps=excluded.steps,
               sleep_h=excluded.sleep_h, weight_kg=excluded.weight_kg, body_fat=excluded.body_fat,
               energy=excluded.energy, note=excluded.note",
            params![day, m.water_ml, m.steps, m.sleep_h, m.weight_kg, m.body_fat, m.energy, m.note],
        )
        .map(|_| ())
    })
}

// ———————————————————————————————————————————————————————————— day + ranges

#[derive(Serialize)]
struct DayView {
    day: String,
    entries: Vec<Entry>,
    workouts: Vec<Workout>,
    metrics: DayMetrics,
}

#[tauri::command]
fn get_day(db: State<Db>, day: String) -> R<DayView> {
    db.with(|c| {
        let mut stmt = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE day = ?1 ORDER BY created_at, id"
        ))?;
        let entries = stmt.query_map([&day], entry_row)?.collect::<rusqlite::Result<_>>()?;
        let workouts = load_workouts(c, "WHERE day = ?1 ORDER BY created_at", &[&day])?;
        let metrics = load_metrics(c, &day)?;
        Ok(DayView { day, entries, workouts, metrics })
    })
}

#[derive(Serialize, Default)]
struct DayStat {
    day: String,
    entries: i64,
    kcal: f64,
    protein: f64,
    carbs: f64,
    fat: f64,
    fiber: f64,
    workouts: i64,
    minutes: f64,
    burned: f64,
    volume: f64,
    water_ml: Option<f64>,
    steps: Option<i64>,
    sleep_h: Option<f64>,
    weight_kg: Option<f64>,
    body_fat: Option<f64>,
    energy: Option<i64>,
}

/// One row per day in [from, to] that has anything logged.
#[tauri::command]
fn range_stats(db: State<Db>, from: String, to: String) -> R<Vec<DayStat>> {
    db.with(|c| {
        let mut map: BTreeMap<String, DayStat> = BTreeMap::new();
        fn slot(map: &mut BTreeMap<String, DayStat>, day: String) -> &mut DayStat {
            map.entry(day.clone()).or_insert_with(|| DayStat { day, ..Default::default() })
        }

        let mut stmt = c.prepare(
            "SELECT day, COUNT(*), SUM(kcal*qty), SUM(protein*qty), SUM(carbs*qty), SUM(fat*qty), SUM(fiber*qty)
             FROM entries WHERE day BETWEEN ?1 AND ?2 GROUP BY day",
        )?;
        let rows = stmt.query_map([&from, &to], |r| {
            Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
        })?;
        for row in rows {
            let (day, n, kcal, p, cb, f, fib): (String, i64, f64, f64, f64, f64, f64) = row?;
            let s = slot(&mut map, day);
            s.entries = n;
            s.kcal = kcal;
            s.protein = p;
            s.carbs = cb;
            s.fat = f;
            s.fiber = fib;
        }

        let mut stmt = c.prepare(
            "SELECT w.day, COUNT(*), SUM(w.duration_min), SUM(COALESCE(w.kcal, 0)),
                    COALESCE(SUM((SELECT SUM(COALESCE(s.reps,0) * COALESCE(s.weight_kg,0)) FROM sets s WHERE s.workout_id = w.id)), 0)
             FROM workouts w WHERE w.day BETWEEN ?1 AND ?2 GROUP BY w.day",
        )?;
        let rows = stmt.query_map([&from, &to], |r| {
            Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?;
        for row in rows {
            let (day, n, mins, burned, vol): (String, i64, f64, f64, f64) = row?;
            let s = slot(&mut map, day);
            s.workouts = n;
            s.minutes = mins;
            s.burned = burned;
            s.volume = vol;
        }

        let mut stmt = c.prepare(
            "SELECT day, water_ml, steps, sleep_h, weight_kg, body_fat, energy FROM days WHERE day BETWEEN ?1 AND ?2",
        )?;
        let rows = stmt.query_map([&from, &to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<f64>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<f64>>(3)?,
                r.get::<_, Option<f64>>(4)?,
                r.get::<_, Option<f64>>(5)?,
                r.get::<_, Option<i64>>(6)?,
            ))
        })?;
        for row in rows {
            let (day, water, steps, sleep, weight, bf, energy) = row?;
            if water.is_none() && steps.is_none() && sleep.is_none() && weight.is_none() && bf.is_none() && energy.is_none() {
                continue;
            }
            let s = slot(&mut map, day);
            s.water_ml = water;
            s.steps = steps;
            s.sleep_h = sleep;
            s.weight_kg = weight;
            s.body_fat = bf;
            s.energy = energy;
        }
        Ok(map.into_values().collect())
    })
}

/// Every weigh-in ever, for the body chart (cheap: one row per day at most).
#[tauri::command]
fn weight_series(db: State<Db>) -> R<Vec<(String, f64)>> {
    db.with(|c| {
        let mut stmt = c.prepare("SELECT day, weight_kg FROM days WHERE weight_kg IS NOT NULL ORDER BY day")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    })
}

/// Every day that has anything logged, for streaks and the year grid.
#[tauri::command]
fn logged_days(db: State<Db>) -> R<Vec<String>> {
    db.with(|c| {
        let mut stmt = c.prepare(
            "SELECT day FROM entries UNION SELECT day FROM workouts
             UNION SELECT day FROM days WHERE water_ml IS NOT NULL OR steps IS NOT NULL OR sleep_h IS NOT NULL OR weight_kg IS NOT NULL
             ORDER BY day",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    })
}

// ———————————————————————————————————————————————————————————— data

#[tauri::command]
fn data_path(db: State<Db>) -> String {
    db.path.display().to_string()
}

fn table_json(c: &Connection, table: &str) -> rusqlite::Result<serde_json::Value> {
    let mut stmt = c.prepare(&format!("SELECT * FROM {table}"))?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt.query_map([], |r| {
        let mut obj = serde_json::Map::new();
        for (i, n) in names.iter().enumerate() {
            let v = match r.get_ref(i)? {
                rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                rusqlite::types::ValueRef::Integer(x) => x.into(),
                rusqlite::types::ValueRef::Real(x) => x.into(),
                rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned().into(),
                rusqlite::types::ValueRef::Blob(_) => serde_json::Value::Null,
            };
            obj.insert(n.clone(), v);
        }
        Ok(serde_json::Value::Object(obj))
    })?;
    Ok(serde_json::Value::Array(rows.collect::<rusqlite::Result<_>>()?))
}

/// Writes everything to a JSON file in Downloads and returns its path.
#[tauri::command]
fn export_data(app: tauri::AppHandle, db: State<Db>, stamp: String) -> R<String> {
    let doc = db.with(|c| {
        let mut out = serde_json::Map::new();
        for t in ["foods", "entries", "workouts", "sets", "days", "settings"] {
            out.insert(t.into(), table_json(c, t)?);
        }
        Ok(serde_json::Value::Object(out))
    })?;
    let dir = app.path().download_dir().map_err(err)?;
    let path = dir.join(format!("tally-export-{stamp}.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(&doc).map_err(err)?).map_err(err)?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn reveal_path(path: String) -> R<()> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg("-R").arg(&path).spawn().map_err(err)?;
    // Explorer wants "/select," and the path as one argument.
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer").arg(format!("/select,{path}")).spawn().map_err(err)?;
    #[cfg(target_os = "linux")]
    {
        let dir = std::path::Path::new(&path).parent().map(|d| d.to_path_buf()).unwrap_or_default();
        std::process::Command::new("xdg-open").arg(dir).spawn().map_err(err)?;
    }
    Ok(())
}

// ———————————————————————————————————————————————————————————— coach

#[derive(Serialize)]
struct ActivePlan {
    id: i64,
    start_day: String,
    created_at: i64,
    plan: coach::Plan,
}

fn active_plan(c: &Connection) -> rusqlite::Result<Option<ActivePlan>> {
    let row = c
        .query_row(
            "SELECT id, start_day, created_at, plan FROM plans WHERE active = 1 ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)),
        )
        .optional()?;
    Ok(row.and_then(|(id, start_day, created_at, json)| {
        serde_json::from_str(&json).ok().map(|plan| ActivePlan { id, start_day, created_at, plan })
    }))
}

/// Preview only: nothing is stored.
#[tauri::command]
fn coach_preview(profile: coach::Profile, year: i32) -> coach::Plan {
    coach::generate(&profile, year)
}

#[tauri::command]
fn coach_save(db: State<Db>, profile: coach::Profile, year: i32, start_day: String) -> R<ActivePlan> {
    let plan = coach::generate(&profile, year);
    let plan_json = serde_json::to_string(&plan).map_err(err)?;
    let profile_json = serde_json::to_string(&plan.profile).map_err(err)?;
    db.with(|c| {
        let tx = c.transaction()?;
        tx.execute("UPDATE plans SET active = 0", [])?;
        tx.execute(
            "INSERT INTO plans (created_at, start_day, profile, plan, active) VALUES (?1, ?2, ?3, ?4, 1)",
            params![now_ms(), start_day, profile_json, plan_json],
        )?;
        tx.commit()?;
        Ok(active_plan(c)?.expect("just inserted"))
    })
}

#[tauri::command]
fn coach_active(db: State<Db>) -> R<Option<ActivePlan>> {
    db.with(|c| active_plan(c))
}

#[tauri::command]
fn coach_clear(db: State<Db>) -> R<()> {
    db.with(|c| c.execute("UPDATE plans SET active = 0", []).map(|_| ()))
}

#[derive(Serialize)]
struct Suggestion {
    last_day: Option<String>,
    last_sets: Vec<(Option<i64>, Option<f64>)>,
    load_kg: Option<f64>,
    reason: String,
}

#[derive(Serialize)]
struct SessionView {
    week: coach::program::Week,
    week_index: usize,
    block_len: usize,
    session_index: Option<usize>,
    /// The day's session with this week's sets and effort applied.
    session: Option<coach::program::Session>,
    suggestions: Vec<Suggestion>,
    next: Option<(String, String)>,
}

/// What the plan says for `day`, with loads suggested from your own history.
#[tauri::command]
fn coach_session(db: State<Db>, day: String) -> R<Option<SessionView>> {
    db.with(|c| session_view(c, &day))
}

fn session_view(c: &Connection, day: &str) -> rusqlite::Result<Option<SessionView>> {
    let day = day.to_string();
    {
        let Some(active) = active_plan(c)? else { return Ok(None) };
        let t = &active.plan.training;
        let start = coach::day_number(&active.start_day).unwrap_or(0);
        let now = coach::day_number(&day).unwrap_or(start);
        let since = (now - start).max(0);
        let week_index = ((since / 7) as usize) % t.block.len();
        let week = t.block[week_index].clone();
        let wd = coach::weekday(&day).unwrap_or(0);
        let session_index = t.schedule.get(wd).copied().flatten();

        let session = session_index.map(|i| {
            let mut s = t.sessions[i].clone();
            for e in &mut s.exercises {
                e.sets = ((e.sets as f64 * week.volume_pct as f64 / 100.0).round() as u8).max(1);
                e.rir = if week.deload { 4 } else if e.tier == 3 { week.rir.saturating_sub(1) } else { week.rir };
            }
            s
        });

        let mut suggestions = Vec::new();
        if let Some(s) = &session {
            let mut last_day_stmt = c.prepare(
                "SELECT w.day, w.id FROM sets s JOIN workouts w ON w.id = s.workout_id
                 WHERE s.exercise = ?1 COLLATE NOCASE AND w.day < ?2 ORDER BY w.day DESC, w.id DESC LIMIT 1",
            )?;
            let mut sets_stmt = c.prepare(
                "SELECT reps, weight_kg FROM sets WHERE workout_id = ?1 AND exercise = ?2 COLLATE NOCASE ORDER BY position",
            )?;
            for e in &s.exercises {
                let last: Option<(String, i64)> = last_day_stmt.query_row(params![e.name, day], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
                let Some((last_day, wid)) = last else {
                    suggestions.push(Suggestion {
                        last_day: None,
                        last_sets: vec![],
                        load_kg: None,
                        reason: if e.step_kg == 0.0 { "Bodyweight: aim for the rep range.".into() } else { "First time: pick a load you could lift for a few more reps than the top of the range.".into() },
                    });
                    continue;
                };
                let last_sets: Vec<(Option<i64>, Option<f64>)> =
                    sets_stmt.query_map(params![wid, e.name], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
                let top = last_sets.iter().filter_map(|s| s.1).fold(0.0_f64, f64::max);
                let at_top: Vec<i64> = last_sets.iter().filter(|s| s.1.unwrap_or(0.0) == top).map(|s| s.0.unwrap_or(0)).collect();
                let hit_top = !at_top.is_empty() && at_top.iter().all(|&r| r >= e.rep_high as i64);
                let missed = at_top.iter().any(|&r| r < e.rep_low as i64);
                let (load, reason) = if e.step_kg == 0.0 || top == 0.0 {
                    (None, if hit_top { "Top of the range last time: slow the tempo or add load.".to_string() } else { "Beat last time's reps.".to_string() })
                } else if week.deload {
                    (Some(((top * 0.9) / e.step_kg).round() * e.step_kg), "Deload week: about 90% of last time, easy sets.".to_string())
                } else if hit_top {
                    (Some(top + e.step_kg), format!("You hit {} reps on every set: add {} kg.", e.rep_high, e.step_kg))
                } else if missed {
                    (Some(top), format!("Below {} reps last time: same load, own the range.", e.rep_low))
                } else {
                    (Some(top), "Same load, add a rep or two.".to_string())
                };
                suggestions.push(Suggestion { last_day: Some(last_day), last_sets, load_kg: load, reason });
            }
        }

        let next = (1..=7).find_map(|k| {
            let d = coach::civil_from_days(now + k);
            let i = t.schedule.get(coach::weekday(&d)?).copied().flatten()?;
            Some((d, t.sessions[i].name.clone()))
        });

        Ok(Some(SessionView { week, week_index, block_len: t.block.len(), session_index, session, suggestions, next }))
    }
}

#[tauri::command]
fn coach_checkin(db: State<Db>, today: String) -> R<Option<coach::checkin::Checkin>> {
    db.with(|c| checkin_for(c, &today))
}

fn checkin_for(c: &Connection, today: &str) -> rusqlite::Result<Option<coach::checkin::Checkin>> {
    {
        let Some(active) = active_plan(c)? else { return Ok(None) };
        let mut stmt = c.prepare("SELECT day, weight_kg FROM days WHERE weight_kg IS NOT NULL")?;
        let weights: Vec<(String, f64)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut stmt = c.prepare("SELECT day, SUM(kcal * qty) FROM entries GROUP BY day")?;
        let intake: Vec<(String, f64)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let raw: Option<String> = c.query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get(0)).optional()?;
        let kcal = raw
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
            .map(|s| s.kcal)
            .unwrap_or(active.plan.nutrition.kcal);
        Ok(Some(coach::checkin::review(&active.plan, &weights, &intake, today, kcal)))
    }
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("tally.db");
            let conn = db::open(&path).map_err(|e| format!("could not open {}: {e}", path.display()))?;
            app.manage(Db { conn: Mutex::new(conn), path });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            search_foods,
            save_food,
            delete_food,
            toggle_favorite,
            add_entry,
            update_entry,
            delete_entry,
            copy_meal,
            save_workout,
            delete_workout,
            list_workouts,
            recent_workouts,
            exercise_names,
            exercise_history,
            exercise_summaries,
            set_metrics,
            get_day,
            range_stats,
            weight_series,
            logged_days,
            data_path,
            export_data,
            reveal_path,
            coach_preview,
            coach_save,
            coach_active,
            coach_session,
            coach_checkin,
            coach_clear,
            recommend_meals,
            edit_entry,
            log_recipe,
            recipe_for,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tally");
}

// ———————————————————————————————————————————————————————————— meal suggestions

fn load_settings(c: &Connection) -> rusqlite::Result<Settings> {
    let raw: Option<String> = c.query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get(0)).optional()?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
}

#[derive(Serialize)]
struct MealSuggestions {
    budget: coach::meals::Budget,
    diet: coach::meals::Diet,
    goal: coach::Goal,
    suggestions: Vec<coach::meals::Suggestion>,
}

#[tauri::command]
fn recommend_meals(db: State<Db>, day: String, slot: coach::meals::Slot, quick: Option<bool>) -> R<MealSuggestions> {
    db.with(|c| meal_suggestions(c, &day, slot, quick.unwrap_or(false)))
}

fn meal_suggestions(c: &Connection, day: &str, slot: coach::meals::Slot, quick: bool) -> rusqlite::Result<MealSuggestions> {
    use coach::meals::{suggest, Context, Diet, Slot};
    let s = load_settings(c)?;

    let mut eaten = (0.0, 0.0);
    let mut logged: Vec<String> = Vec::new();
    {
        let mut stmt = c.prepare("SELECT meal, kcal * qty, protein * qty FROM entries WHERE day = ?1")?;
        let rows = stmt.query_map([day], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, f64>(2)?)))?;
        for row in rows {
            let (meal, k, p) = row?;
            eaten.0 += k;
            eaten.1 += p;
            logged.push(meal);
        }
    }
    let open: Vec<Slot> = [Slot::Breakfast, Slot::Lunch, Slot::Dinner, Slot::Snack]
        .into_iter()
        .filter(|x| *x == slot || !logged.iter().any(|m| m == x.label()))
        .collect();

    let goal = match active_plan(c)? {
        Some(a) => a.plan.profile.goal,
        None if s.pace_kg_week < -0.05 => coach::Goal::Cut,
        None if s.pace_kg_week > 0.05 => coach::Goal::LeanBulk,
        None => coach::Goal::Maintain,
    };

    // Dishes logged in the last three days, to keep suggestions varied.
    let today_n = coach::day_number(day).unwrap_or(0);
    let mut stmt = c.prepare("SELECT DISTINCT lower(name) FROM entries WHERE day BETWEEN ?1 AND ?2")?;
    let recent: Vec<String> = stmt
        .query_map(params![coach::civil_from_days(today_n - 3), day], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;

    let diet = Diet::parse(&s.diet);
    let cx = Context {
        slot,
        open_slots: &open,
        kcal_left: s.kcal - eaten.0,
        protein_left: s.protein - eaten.1,
        goal,
        diet,
        recent: &recent,
        max_minutes: quick.then_some(10),
    };
    let (budget, suggestions) = suggest(&cx, 6);
    Ok(MealSuggestions { budget, diet, goal, suggestions })
}

/// The recipe behind a logged entry's name, if it came from the suggestions.
#[tauri::command]
fn recipe_for(name: String) -> Option<coach::meals::RecipeView> {
    coach::meals::by_name(&name).map(coach::meals::view)
}

/// Log a suggested dish. The recipe is kept in the food library (one entry
/// per recipe) so it shows up in search and recents afterwards.
#[tauri::command]
fn log_recipe(db: State<Db>, day: String, meal: String, recipe_id: String, portion: f64) -> R<Entry> {
    db.with(|c| insert_recipe(c, &day, &meal, &recipe_id, portion))
}

fn insert_recipe(c: &mut Connection, day: &str, meal: &str, recipe_id: &str, portion: f64) -> rusqlite::Result<Entry> {
    let r = coach::meals::by_id(recipe_id).ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    let tx = c.transaction()?;
    let now = now_ms();
    let existing: Option<i64> = tx
        .query_row("SELECT id FROM foods WHERE name = ?1 AND archived = 0 ORDER BY id LIMIT 1", [r.name], |row| row.get(0))
        .optional()?;
    let food_id = match existing {
        Some(id) => id,
        None => {
            tx.execute(
                "INSERT INTO foods (name, serving, kcal, protein, carbs, fat, fiber, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![r.name, r.serving, r.kcal, r.protein, r.carbs, r.fat, r.fiber, now],
            )?;
            tx.last_insert_rowid()
        }
    };
    tx.execute("UPDATE foods SET use_count = use_count + 1, last_used_at = ?1 WHERE id = ?2", params![now, food_id])?;
    tx.execute(
        "INSERT INTO entries (day, meal, food_id, name, serving, qty, kcal, protein, carbs, fat, fiber, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![day, meal, food_id, r.name, r.serving, portion, r.kcal, r.protein, r.carbs, r.fat, r.fiber, now],
    )?;
    let id = tx.last_insert_rowid();
    let e = tx.query_row(&format!("SELECT {ENTRY_COLS} FROM entries WHERE id = ?1"), [id], entry_row)?;
    tx.commit()?;
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        db::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn save_plan(c: &Connection, start: &str) -> coach::Plan {
        let mut p: coach::Profile = serde_json::from_value(serde_json::json!({
            "sex": "male", "birth_year": 1996, "height_cm": 178.0, "weight_kg": 80.0, "goal": "lean_bulk", "pace": "standard",
            "experience": "intermediate", "days": 4, "session_min": 75, "equipment": "gym", "lifestyle": "desk"
        }))
        .unwrap();
        p.focus.clear();
        let plan = coach::generate(&p, 2026);
        c.execute(
            "INSERT INTO plans (created_at, start_day, profile, plan, active) VALUES (0, ?1, '{}', ?2, 1)",
            params![start, serde_json::to_string(&plan).unwrap()],
        )
        .unwrap();
        plan
    }

    #[test]
    fn session_suggests_load_from_history() {
        let c = mem();
        save_plan(&c, "2026-10-26");
        // Last Monday's bench: all sets at the top of the range.
        c.execute("INSERT INTO workouts (day, title, kind, duration_min, created_at) VALUES ('2026-10-26', 'Upper A', 'strength', 70, 0)", []).unwrap();
        let wid = c.last_insert_rowid();
        for i in 0..3 {
            c.execute("INSERT INTO sets (workout_id, position, exercise, reps, weight_kg) VALUES (?1, ?2, 'Bench press', 8, 80)", params![wid, i]).unwrap();
        }
        // 2026-11-02 is a Monday: Upper A, week 2 of the block.
        let v = session_view(&c, "2026-11-02").unwrap().unwrap();
        assert_eq!(v.week_index, 1);
        let s = v.session.unwrap();
        assert_eq!(s.name, "Upper A");
        let i = s.exercises.iter().position(|e| e.name == "Bench press").unwrap();
        assert_eq!(v.suggestions[i].load_kg, Some(82.5));
        assert_eq!(v.suggestions[i].last_day.as_deref(), Some("2026-10-26"));

        // Sunday is a rest day; the next session is Monday's.
        let rest = session_view(&c, "2026-11-01").unwrap().unwrap();
        assert!(rest.session.is_none());
        assert_eq!(rest.next.unwrap().0, "2026-11-02");
    }

    #[test]
    fn checkin_reads_weights_and_intake() {
        let c = mem();
        save_plan(&c, "2026-10-01");
        for i in 0..20 {
            let d = coach::civil_from_days(coach::day_number("2026-10-12").unwrap() + i);
            c.execute("INSERT INTO days (day, weight_kg) VALUES (?1, 80.0)", params![d]).unwrap();
            c.execute(
                "INSERT INTO entries (day, meal, name, serving, qty, kcal, protein, carbs, fat, created_at) VALUES (?1, 'lunch', 'x', '1', 1, 2500, 0, 0, 0, 0)",
                params![d],
            )
            .unwrap();
        }
        let ck = checkin_for(&c, "2026-10-31").unwrap().unwrap();
        assert!(ck.weigh_ins >= 6);
        assert_eq!(ck.avg_intake, Some(2500.0));
        assert!(ck.observed_tdee.is_some());
    }

    #[test]
    fn suggestions_use_what_is_left_and_logging_reuses_the_food() {
        let mut c = mem();
        // Breakfast already logged: 500 kcal of a 2200 default.
        c.execute(
            "INSERT INTO entries (day, meal, name, serving, qty, kcal, protein, carbs, fat, created_at) VALUES ('2026-11-16', 'breakfast', 'x', '1', 1, 500, 30, 0, 0, 0)",
            [],
        )
        .unwrap();
        let m = meal_suggestions(&c, "2026-11-16", coach::meals::Slot::Lunch, false).unwrap();
        assert_eq!(m.budget.kcal_left, 1700.0);
        // Lunch takes 0.35 of the 0.75 still open (lunch, dinner, snack).
        assert_eq!(m.budget.kcal, (1700.0_f64 * 0.35 / 0.75).round());
        let top = &m.suggestions[0];
        insert_recipe(&mut c, "2026-11-16", "lunch", &top.id, top.portion).unwrap();
        insert_recipe(&mut c, "2026-11-17", "lunch", &top.id, 1.0).unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM foods WHERE name = ?1", [&top.name], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        // Having it today pushes it down tomorrow.
        let next = meal_suggestions(&c, "2026-11-17", coach::meals::Slot::Lunch, false).unwrap();
        assert_ne!(next.suggestions[0].id, top.id);
    }

    #[test]
    fn editing_an_entry_and_its_food() {
        let mut c = mem();
        let e = insert_recipe(&mut c, "2026-11-16", "breakfast", "masala_omelette", 1.0).unwrap();
        let edit = EntryEdit {
            name: "Masala omelette (no oil)".into(),
            serving: e.serving.clone(),
            meal: "breakfast".into(),
            qty: 1.5,
            kcal: e.kcal - 40.0,
            protein: e.protein,
            carbs: e.carbs,
            fat: e.fat - 4.5,
            fiber: e.fiber,
        };
        let out = apply_entry_edit(&mut c, e.id, &edit, false).unwrap();
        assert_eq!(out.qty, 1.5);
        assert_eq!(out.fat, e.fat - 4.5);
        // Library food untouched unless asked.
        let fat: f64 = c.query_row("SELECT fat FROM foods WHERE id = ?1", [e.food_id.unwrap()], |r| r.get(0)).unwrap();
        assert_eq!(fat, e.fat);
        apply_entry_edit(&mut c, e.id, &edit, true).unwrap();
        let (name, fat): (String, f64) = c.query_row("SELECT name, fat FROM foods WHERE id = ?1", [e.food_id.unwrap()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((name.as_str(), fat), ("Masala omelette (no oil)", e.fat - 4.5));
    }

    #[test]
    fn no_plan_no_session() {
        let c = mem();
        assert!(session_view(&c, "2026-11-02").unwrap().is_none());
    }
}
