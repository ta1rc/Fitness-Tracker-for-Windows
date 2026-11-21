<div align="center">

# Tally

**A calm, private, native macOS fitness tracker with a built-in coach.**

Food and macros, workouts and sets, water, steps, sleep, weight and mood, all measured against goals you set.
Everything stays on your Mac in a single SQLite file. No account, no cloud, no network.

![Tauri](https://img.shields.io/badge/Tauri_2-24C8DB?style=flat-square&logo=tauri&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust&logoColor=white)
![React](https://img.shields.io/badge/React_18-20232A?style=flat-square&logo=react&logoColor=61DAFB)
![TypeScript](https://img.shields.io/badge/TypeScript-3178C6?style=flat-square&logo=typescript&logoColor=white)
![Tailwind CSS](https://img.shields.io/badge/Tailwind-06B6D4?style=flat-square&logo=tailwindcss&logoColor=white)
![SQLite](https://img.shields.io/badge/SQLite-003B57?style=flat-square&logo=sqlite&logoColor=white)
![macOS](https://img.shields.io/badge/macOS-000000?style=flat-square&logo=apple&logoColor=white)

<br/>

<img src="docs/screenshots/today.png" alt="Tally: the Today screen" width="100%"/>

</div>

---

## Highlights

- **A coach that shows its reasoning.** An eight-question intake produces calorie and macro targets, a training split and a periodised block. Every number comes with the reasoning behind it.
- **"What to eat now."** Meal suggestions sized to the calories and protein you have left today, with recipes scaled to the portion.
- **Honest progress.** Weight is shown as a smoothed trend, so a salty dinner doesn't look like a setback. Strength progress is tracked as an estimated 1RM for each exercise.
- **Deterministic by design.** The planner is pure Rust with no model calls and no randomness. The same answers always produce the same plan, and `cargo test` checks that.
- **Private by default.** One local SQLite file, with JSON export whenever you want your data.
- **Built to feel native.** An overlay title bar, a living ambient background and keyboard shortcuts for everything.

---

## Screenshots

### Program: the coach

<img src="docs/screenshots/program.png" alt="Program screen with training block, today's session and fuel targets" width="100%"/>

The intake builds a split (here, upper/lower plus push/pull/legs), a five-week block with reps-in-reserve targets and a deload, and today's session. Suggested loads come from your history.

### Food: what to eat now

<img src="docs/screenshots/food.png" alt="Meal suggestions sized to the remaining calories and protein" width="100%"/>

Suggestions are scored on calorie fit, protein fit, protein density for your goal, fibre and variety. Portions scale in quarters. **Log** adds the dish to your diary, and **Recipe** opens measured ingredients and a step-by-step method.

<table>
  <tr>
    <td width="50%">
      <h3>Training</h3>
      <img src="docs/screenshots/training.png" alt="Training screen"/>
      <p>Weekly goal ring, minutes per week, history grouped by week, and estimated 1RM progress for each exercise.</p>
    </td>
    <td width="50%">
      <h3>Body</h3>
      <img src="docs/screenshots/body.png" alt="Body screen with weight trend"/>
      <p>A smoothed weight trend against your goal, with the weekly rate, a projected date and a weigh-in log.</p>
    </td>
  </tr>
  <tr>
    <td width="50%">
      <h3>Trends</h3>
      <img src="docs/screenshots/trends.png" alt="Trends screen"/>
      <p>Week, month and quarter averages for calories, protein, steps, sleep and water, plus a year-long consistency grid.</p>
    </td>
    <td width="50%">
      <h3>Settings</h3>
      <img src="docs/screenshots/settings.png" alt="Goals and settings screen"/>
      <p>Nutrition and habit targets, a Mifflin–St Jeor energy calculator, units and JSON export.</p>
    </td>
  </tr>
</table>

### Ambience

<table>
  <tr>
    <td width="33%"><img src="docs/screenshots/today.png" alt="Paper scene"/></td>
    <td width="33%"><img src="docs/screenshots/today-mist.png" alt="Mist scene"/></td>
    <td width="33%"><img src="docs/screenshots/today-graphite.png" alt="Graphite scene"/></td>
  </tr>
  <tr>
    <td align="center"><sub><b>Paper</b></sub></td>
    <td align="center"><sub><b>Mist</b></sub></td>
    <td align="center"><sub><b>Graphite</b></sub></td>
  </tr>
</table>

The window background is a living scene: Paper, Mist, Dusk, Tide, Graphite, Ember, or **Auto**, which follows the time of day. With **Reactive** on, the scene warms toward the accent colour as the day's food, protein, water, steps and training fill in. Press <kbd>⌘</kbd> <kbd>B</kbd> to cycle scenes.

---

## Features

| Screen | What it does |
|---|---|
| **Today** | Calories left with a macro-split bar, protein/carbs/fat, the week strip and streak, water/steps/sleep/weight tiles, meals, training, and a mood and note check-in |
| **Program** | The coach: intake, today's session with suggested loads, the training block, weekly sets per muscle, a fortnightly check-in that adjusts calories, and the reasoning behind every number |
| **Food** | Meal suggestions, the day's diary (click any entry to edit it), 7-day averages, calorie history, and a food library with favourites and archive |
| **Training** | Editable workouts (title, date, type, duration, effort and every set) with a before/after summary and undo, plus weekly volume and 1RM progress |
| **Body** | Weight trend, weekly rate, projected goal date and weigh-in log |
| **Trends** | Period averages, charts for every habit, and a consistency grid |
| **Settings** | Targets, the energy calculator, units, and data export |

---

## How the coach works

The coach lives in [`src-tauri/src/coach/`](src-tauri/src/coach/). It's a deterministic planner written in Rust.

**Nutrition** ([`nutrition.rs`](src-tauri/src/coach/nutrition.rs))
- Estimates resting energy with Mifflin–St Jeor, or Katch–McArdle when you give a body-fat percentage, then adds daily movement and training cost.
- Scales the rate of change with your goal, pace and training age.
- Sets protein by goal (from lean mass when known), then fills the rest with fat and carbs. Training days get slightly more carbs than rest days.

**Programming** ([`program.rs`](src-tauri/src/coach/program.rs), [`library.rs`](src-tauri/src/coach/library.rs))
- Picks a split by days per week: full body ×2 or ×3, upper/lower, upper/lower plus PPL, or PPL ×2.
- Fills each movement slot from an 80-exercise library, based on your equipment and joint limits.
- Spreads weekly set targets across the slots. Focus areas get extra direct sets and an added isolation exercise.
- Trims sessions to fit the time you have, taking sets first from the muscles furthest over their target.
- Builds a training block with reps-in-reserve targets and a deload week.

**Check-ins** ([`checkin.rs`](src-tauri/src/coach/checkin.rs))
- Smooths your weigh-ins, fits a least-squares slope to the last 21 days, and compares it with the planned rate.
- Suggests a calorie change, capped at ±250 kcal.
- When enough food is logged, estimates your real maintenance.

**Meal suggestions** ([`meals.rs`](src-tauri/src/coach/meals.rs))
- Draws from a curated list of about 50 home-cooked dishes, Indian staples included, each with typical macros.
- Works out the current meal from the clock and what you've already logged (at 11:00 with no breakfast, that's breakfast), then gives it its share of the day's remaining calories and protein.
- Filters by diet (any, pescatarian, eggetarian, vegetarian, vegan) and scores each dish on calorie fit, protein fit, protein density weighted by goal, fibre and variety.
- Moves dishes eaten in the last three days down the list, and stops any single kind of dish from crowding the top six.

---

## Tech stack

| Layer | Technology |
|---|---|
| Shell | **Tauri 2**: a Rust core with a native macOS window and overlay title bar |
| Data | **Rust + rusqlite** (bundled SQLite). Every query lives in Rust and is exposed as typed commands in `src-tauri/src/lib.rs`. The schema and starter foods are in `db.rs` |
| UI | **React 18 + TypeScript + Tailwind CSS** |
| Charts | Hand-drawn SVG and CSS in `src/components/charts.tsx`, with no chart library |

---

## Getting started

**Requirements:** macOS, Node 18+ and a [Rust toolchain](https://rustup.rs).

```bash
git clone https://github.com/ta1rc/Fitness-Tracker-.git
cd Fitness-Tracker-
npm install
```

| Command | What it does |
|---|---|
| `npm run tauri dev` | Runs the desktop app with hot reload |
| `npm run tauri build` | Builds `Tally.app` and a `.dmg` into `src-tauri/target/release/bundle/` |
| `npm run dev` | Runs the UI only, in a browser, against an in-memory mock (`src/dev/mock.ts`) |
| `cargo test` | Runs the coach tests (from `src-tauri/`) |

---

## Keyboard shortcuts

| Keys | Action |
|---|---|
| <kbd>⌘</kbd> <kbd>K</kbd> | Quick log: search a food to add one serving, or run an action |
| <kbd>F</kbd> / <kbd>⌘</kbd> <kbd>N</kbd> | Log food (meal picked by time of day) |
| <kbd>W</kbd> | Log a workout |
| <kbd>←</kbd> <kbd>→</kbd> or <kbd>[</kbd> <kbd>]</kbd> | Previous / next day |
| <kbd>T</kbd> | Jump to today |
| <kbd>⌘</kbd> <kbd>1</kbd> … <kbd>⌘</kbd> <kbd>7</kbd> | Switch screen |
| <kbd>⌘</kbd> <kbd>B</kbd> | Next ambience scene |
| <kbd>↑</kbd> <kbd>↓</kbd> in number fields | Nudge the value (hold <kbd>⇧</kbd> for ×10) |

---

## Data and privacy

- All data lives in one file: `~/Library/Application Support/app.tally.desktop/tally.db`.
- There are no accounts, no telemetry and no network requests.
- Export everything as JSON from **Settings → Your data**.

## Design notes

- **History is immutable.** Diary entries copy per-serving values when you log them, so editing a food later doesn't rewrite past days.
- **Trend over noise.** Weight is shown as an exponential moving average (α = 0.1). The weekly rate is a least-squares fit over the last 28 days of that trend.

## Windows

Tally runs on Windows too: Tauri, Rust and the bundled SQLite are all cross-platform. On Windows the data lives in `%APPDATA%\app.tally.desktop\tally.db`.

**Download:** grab `Tally_<version>_x64-setup.exe` from the [Releases](../../releases) page and run it. The installer isn't code-signed, so Windows SmartScreen asks once: **More info → Run anyway**. If WebView2 is missing (it ships with Windows 10 and 11), the installer fetches it.

**Build it yourself on a Mac.** This uses Tauri's cross-compilation, which is experimental and makes an NSIS `.exe` only. Set up once:

```bash
brew install makensis llvm lld
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
```

Then build. The `LANG`/`LC_ALL` line matters: NSIS crashes on macOS when no UTF-8 locale is set, and LLVM's tools need to be on the `PATH`:

```bash
export LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 PATH="$(brew --prefix llvm)/bin:$PATH"
npm run tauri build -- --runner cargo-xwin --target x86_64-pc-windows-msvc
```

The installer lands in `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`. It needs nothing beyond Windows 10 or 11: no Visual C++ redistributable. On the first run, `cargo-xwin` downloads Microsoft's Windows SDK and CRT files.

**Or on a Windows PC:** install Node 18+, Rust and the Visual Studio Build Tools ("Desktop development with C++"), then run `npm install` and `npm run tauri build`.

On Windows, shortcuts use Ctrl wherever this README says ⌘.
