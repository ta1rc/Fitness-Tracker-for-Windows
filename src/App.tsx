import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, initBackend, isTauri, MEAL_LABEL, totals, type Food } from "./api";
import { useData, useSettings, useStore, type View } from "./store";
import { addDays, greeting, longDate, mealForNow, parseDay, relativeDay, today } from "./lib/date";
import { int, round, toKg, weightUnit } from "./lib/units";
import { Today } from "./views/Today";
import { Program } from "./views/Program";
import { Nutrition } from "./views/Nutrition";
import { Training } from "./views/Training";
import { Body } from "./views/Body";
import { Trends } from "./views/Trends";
import { Goals } from "./views/Goals";
import { FoodSheet } from "./views/FoodSheet";
import { WorkoutSheet } from "./views/WorkoutSheet";
import { EntrySheet } from "./views/EntrySheet";
import { RecipeSheet } from "./views/RecipeSheet";
import { Ambient, SCENES, Swatch, effectiveScene, type SceneId } from "./components/Ambient";
import { Field, NumberField, Segmented } from "./components/ui";
import { IChevL, IChevR, IDrop, INutrition, IPlus, ISearch } from "./components/icons";

const NAV: { id: View; label: string; dated?: boolean }[] = [
  { id: "today", label: "Today", dated: true },
  { id: "program", label: "Program" },
  { id: "nutrition", label: "Food", dated: true },
  { id: "training", label: "Training" },
  { id: "body", label: "Body" },
  { id: "trends", label: "Trends" },
  { id: "goals", label: "Settings" },
];

const isMac = navigator.userAgent.includes("Mac");

const SCENE_ORDER: (SceneId | "auto")[] = ["auto", ...SCENES.map((s) => s.id)];

const typing = (e: KeyboardEvent) => {
  const t = e.target as HTMLElement;
  return t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable;
};

export default function App() {
  const settings = useStore((s) => s.settings);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri) document.documentElement.classList.add("web");
    void initBackend().then((ok) => {
      if (!ok) return setError("Tally runs as a desktop app. Start it with `npm run tauri dev`.");
      api.getSettings().then((s) => useStore.setState({ settings: s }), (e) => setError(String(e)));
    });
  }, []);

  // The date rolls over while the app is open: follow it if we were on today.
  useEffect(() => {
    let last = today();
    const id = setInterval(() => {
      const t = today();
      if (t !== last) {
        const st = useStore.getState();
        if (st.day === last) st.setDay(t);
        last = t;
        st.bump();
      }
    }, 30_000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const st = useStore.getState();
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key === "k") return e.preventDefault(), st.setPalette(!st.palette);
      if (mod && e.key === "b" && st.settings) {
        e.preventDefault();
        const i = SCENE_ORDER.indexOf(st.settings.scene as SceneId);
        return void st.updateSettings({ scene: SCENE_ORDER[(i + 1) % SCENE_ORDER.length] });
      }
      if (st.sheet || st.palette) return;
      if (mod && /^[1-7]$/.test(e.key)) return e.preventDefault(), st.go(NAV[Number(e.key) - 1].id);
      if (mod && e.key === "n") return e.preventDefault(), st.open({ type: "food", meal: mealForNow(), day: st.day });
      if (typing(e) || mod || e.altKey) return;
      if (e.key === "ArrowLeft" || e.key === "[") return e.preventDefault(), st.shiftDay(-1);
      if (e.key === "ArrowRight" || e.key === "]") return e.preventDefault(), st.shiftDay(1);
      if (e.key === "t") return st.setDay(today());
      if (e.key === "f") return e.preventDefault(), st.open({ type: "food", meal: mealForNow(), day: st.day });
      if (e.key === "w") return e.preventDefault(), st.open({ type: "workout", day: st.day });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (error)
    return (
      <div className="flex h-full items-center justify-center p-10" style={{ background: "#ebe6de", color: "#141311" }}>
        <div className="max-w-md">
          <p className="label">Error</p>
          <h1 className="title mt-2 text-[22px]">Tally couldn't start</h1>
          <p className="mt-2 text-[13px] leading-relaxed opacity-70">{error}</p>
        </div>
      </div>
    );
  if (!settings) return null;
  return <Shell />;
}

function Shell() {
  const view = useStore((s) => s.view);
  const sheet = useStore((s) => s.sheet);
  const palette = useStore((s) => s.palette);
  const settings = useSettings();
  const scrollRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: 0 });
  }, [view]);

  return (
    <>
      <Ambient />
      <div className="relative z-10 flex h-full">
        <Rail />
        <main className="flex min-w-0 flex-1 flex-col py-3 pr-3">
          <div className="plate relative flex min-h-0 flex-1 flex-col overflow-hidden">
            <Header />
            <div ref={scrollRef} className="scroll min-h-0 flex-1 px-8 pb-10">
              <div key={view} className="mx-auto max-w-[1240px]">
                {view === "today" && <Today />}
                {view === "program" && <Program />}
                {view === "nutrition" && <Nutrition />}
                {view === "training" && <Training />}
                {view === "body" && <Body />}
                {view === "trends" && <Trends />}
                {view === "goals" && <Goals />}
              </div>
            </div>
          </div>
        </main>
        {sheet?.type === "food" && <FoodSheet key={sheet.meal + sheet.day} meal={sheet.meal} day={sheet.day} />}
        {sheet?.type === "recipe" && <RecipeSheet key={sheet.recipe.id} recipe={sheet.recipe} portion={sheet.portion} logTo={sheet.logTo} />}
        {sheet?.type === "entry" && <EntrySheet key={sheet.entry.id} entry={sheet.entry} />}
        {sheet?.type === "workout" && <WorkoutSheet day={sheet.day} workout={sheet.workout} template={sheet.template} />}
        {palette && <Palette />}
        {!settings.onboarded && <Welcome />}
        <Toast />
      </div>
    </>
  );
}

// ———————————————————————————————————————————————————————————— rail

function Rail() {
  const view = useStore((s) => s.view);
  const go = useStore((s) => s.go);
  const s = useSettings();
  const t = today();
  const data = useData(() => api.getDay(t), [t]);
  const m = data ? totals(data.entries) : null;
  const left = m ? s.kcal - m.kcal : null;

  return (
    <aside className="flex w-[228px] shrink-0 flex-col pl-6 pr-4">
      {/* Room for the macOS window buttons; Windows and Linux have a regular title bar. */}
      <div data-tauri-drag-region className={`${isMac ? "h-[58px]" : "h-5"} shrink-0`} />
      <div data-tauri-drag-region className="flex items-center gap-2.5 pb-9 pt-1">
        <Logo />
        <span className="title text-[17px]">Tally</span>
      </div>

      <nav className="space-y-[3px]">
        {NAV.map((n, i) => {
          const on = view === n.id;
          return (
            <button key={n.id} onClick={() => go(n.id)} className="group flex h-8 w-full items-center gap-3.5 text-left">
              <span className="mono w-5 text-[10.5px] transition-colors" style={{ color: on ? "var(--accent)" : "var(--ink-3)" }}>
                {String(i + 1).padStart(2, "0")}
              </span>
              <span
                className="text-[14.5px] transition-colors group-hover:text-ink"
                style={{ color: on ? "var(--ink)" : "var(--ink-2)", fontWeight: on ? 560 : 450, letterSpacing: "-0.01em" }}
              >
                {n.label}
              </span>
              <span className="ml-auto h-px flex-1 origin-left transition-transform duration-500" style={{ background: "var(--ink-3)", transform: `scaleX(${on ? 1 : 0})`, maxWidth: 40 }} />
            </button>
          );
        })}
      </nav>

      <div className="mt-10">
        <div className="label">Remaining today</div>
        <div className="num display mt-2 text-[38px] leading-none" style={{ color: left != null && left < 0 ? "var(--danger)" : undefined }}>
          {left == null ? "—" : int(Math.abs(left))}
        </div>
        <div className="mono mt-1.5 text-[10.5px] text-ink3">{left != null && left < 0 ? "KCAL OVER" : "KCAL"}</div>
        {m && (
          <div className="mt-4 space-y-2.5">
            <RailBar label="Protein" v={m.protein} max={s.protein} color="var(--protein)" />
            <RailBar label="Water" v={data?.metrics.water_ml ?? 0} max={s.water_ml} color="var(--water)" />
            <RailBar label="Steps" v={data?.metrics.steps ?? 0} max={s.steps} color="var(--steps)" />
          </div>
        )}
      </div>

      <div className="mt-auto pb-6">
        <ScenePicker />
        <button className="mt-5 flex w-full items-center gap-2 text-[12px] text-ink3 transition-colors hover:text-ink2" onClick={() => useStore.getState().setPalette(true)}>
          <ISearch size={12} /> Quick log <span className="kbd ml-auto">⌘K</span>
        </button>
      </div>
    </aside>
  );
}

function RailBar({ label, v, max, color }: { label: string; v: number; max: number; color: string }) {
  const pct = Math.min(1, v / (max || 1));
  return (
    <div>
      <div className="mono mb-1 flex justify-between text-[10px] uppercase tracking-[0.06em] text-ink3">
        <span>{label}</span>
        <span className="num">{Math.round(pct * 100)}%</span>
      </div>
      <div className="h-[2px] overflow-hidden rounded-full" style={{ background: "var(--line)" }}>
        <div className="bar-grow h-full" style={{ width: `${pct * 100}%`, background: color }} />
      </div>
    </div>
  );
}

function ScenePicker() {
  const s = useSettings();
  const update = useStore((x) => x.updateSettings);
  const current = effectiveScene(s.scene);
  const name = s.scene === "auto" ? `Auto · ${SCENES.find((x) => x.id === current)!.name}` : SCENES.find((x) => x.id === s.scene)?.name;
  return (
    <div>
      <div className="flex items-baseline justify-between">
        <span className="label">Ambience</span>
        <span className="mono text-[10px] text-ink3">⌘B</span>
      </div>
      <div className="mt-2.5 flex items-center gap-[7px]">
        {SCENE_ORDER.map((id) => {
          const on = s.scene === id;
          return (
            <button
              key={id}
              onClick={() => void update({ scene: id })}
              className="rounded-full p-[2px] transition-shadow"
              style={{ boxShadow: on ? "0 0 0 1.5px var(--ink)" : "0 0 0 1px transparent" }}
              title={id === "auto" ? "Auto: follows the time of day" : SCENES.find((x) => x.id === id)!.name}
            >
              <Swatch id={id} size={17} />
            </button>
          );
        })}
      </div>
      <div className="mt-2.5 flex items-center justify-between text-[12px]">
        <span className="text-ink2">{name}</span>
        <button
          onClick={() => void update({ reactive: !s.reactive })}
          className="mono flex items-center gap-1.5 text-[10px] uppercase tracking-[0.06em]"
          style={{ color: s.reactive ? "var(--ink-2)" : "var(--ink-3)" }}
          title="The background warms as you hit the day's targets"
        >
          <span className="relative inline-block h-[12px] w-[22px] rounded-full transition-colors" style={{ background: s.reactive ? "var(--accent)" : "var(--line-2)" }}>
            <span className="absolute top-[2px] h-2 w-2 rounded-full bg-white transition-all" style={{ left: s.reactive ? 12 : 2 }} />
          </span>
          Reactive
        </button>
      </div>
    </div>
  );
}

function Logo() {
  return (
    <svg width="22" height="22" viewBox="0 0 22 22" aria-hidden>
      <rect width="22" height="22" rx="6" fill="var(--ink)" />
      <g stroke="var(--surface-solid)" strokeWidth="1.6" strokeLinecap="round">
        <path d="M6.6 6.5v9M9.5 6.5v9M12.5 6.5v9M15.4 6.5v9" />
      </g>
      <path d="M4.8 13.9 17.2 8.1" stroke="var(--accent)" strokeWidth="1.9" strokeLinecap="round" />
    </svg>
  );
}

// ———————————————————————————————————————————————————————————— header

function isoWeek(day: string): number {
  const d = parseDay(day);
  d.setDate(d.getDate() + 3 - ((d.getDay() + 6) % 7));
  const w1 = new Date(d.getFullYear(), 0, 4);
  return 1 + Math.round(((d.getTime() - w1.getTime()) / 86_400_000 - 3 + ((w1.getDay() + 6) % 7)) / 7);
}

function Header() {
  const view = useStore((s) => s.view);
  const day = useStore((s) => s.day);
  const { shiftDay, setDay, open } = useStore.getState();
  const s = useSettings();
  const nav = NAV.find((n) => n.id === view)!;
  const idx = NAV.indexOf(nav) + 1;
  const isToday = day === today();

  return (
    <header
      data-tauri-drag-region
      onDoubleClick={(e) => e.target === e.currentTarget && isTauri && void getCurrentWindow().toggleMaximize()}
      className="flex shrink-0 items-end justify-between gap-6 px-8 pb-6 pt-7"
    >
      <div data-tauri-drag-region className="min-w-0">
        <div data-tauri-drag-region className="label flex items-center gap-3">
          <span style={{ color: "var(--accent)" }}>{String(idx).padStart(2, "0")}</span>
          <span>{nav.label}</span>
          <span className="h-px w-6" style={{ background: "var(--line-2)" }} />
          <span>{nav.dated ? (isToday ? `${greeting()}${s.name ? `, ${s.name}` : ""}` : relativeDay(day)) : `Week ${isoWeek(today())}`}</span>
        </div>
        {nav.dated ? (
          <div className="mt-2.5 flex items-center gap-4">
            <h1 data-tauri-drag-region className="title text-[30px] leading-none">
              {longDate(day)}
            </h1>
            <div className="flex items-center gap-0.5">
              <button className="btn btn-icon btn-sm" onClick={() => shiftDay(-1)} aria-label="Previous day" title="Previous day (←)">
                <IChevL size={13} />
              </button>
              <button className="btn btn-icon btn-sm" disabled={isToday} onClick={() => shiftDay(1)} aria-label="Next day" title="Next day (→)">
                <IChevR size={13} />
              </button>
              {!isToday && (
                <button className="btn btn-sm anim-fade ml-1 btn-outline" onClick={() => setDay(today())}>
                  Today
                </button>
              )}
            </div>
          </div>
        ) : (
          <h1 data-tauri-drag-region className="title mt-2.5 text-[30px] leading-none">
            {nav.label === "Settings" ? "Goals & settings" : nav.label}
          </h1>
        )}
      </div>
      <div className="flex items-center gap-2">
        <button className="btn btn-outline" onClick={() => open({ type: "workout", day })} title="Log workout (W)">
          Log workout
        </button>
        <button className="btn btn-primary" onClick={() => open({ type: "food", meal: mealForNow(), day })} title="Log food (F)">
          <IPlus size={13} /> Food
        </button>
      </div>
    </header>
  );
}

// ———————————————————————————————————————————————————————————— palette

type Item = { id: string; label: string; hint?: string; group: string; run: () => void | Promise<void> };

function Palette() {
  const st = useStore.getState();
  const s = useSettings();
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const foods = useData(() => (q.trim() ? api.searchFoods(q, 7) : Promise.resolve([] as Food[])), [q]) ?? [];
  const meal = mealForNow();
  const close = () => st.setPalette(false);

  const addWater = async () => {
    const d = await api.getDay(st.day);
    await api.setMetrics(st.day, { ...d.metrics, water_ml: (d.metrics.water_ml ?? 0) + (s.units === "imperial" ? 236.6 : 250) });
    st.bump();
    st.notify("Glass of water added");
  };

  const actions: Item[] = [
    { id: "food", group: "Log", label: `Log food to ${MEAL_LABEL[meal]}`, hint: "F", run: () => st.open({ type: "food", meal, day: st.day }) },
    { id: "workout", group: "Log", label: "Log a workout", hint: "W", run: () => st.open({ type: "workout", day: st.day }) },
    { id: "water", group: "Log", label: "Add a glass of water", run: addWater },
    { id: "weigh", group: "Log", label: "Log weight", run: () => st.go("body") },
    ...NAV.map((n, i) => ({ id: `go-${n.id}`, group: "Go to", label: n.label, hint: `⌘${i + 1}`, run: () => st.go(n.id) })),
    { id: "yesterday", group: "Go to", label: "Yesterday", run: () => st.setDay(addDays(today(), -1)) },
    ...SCENE_ORDER.map((id) => ({
      id: `scene-${id}`,
      group: "Ambience",
      label: id === "auto" ? "Auto ambience" : `${SCENES.find((x) => x.id === id)!.name} ambience`,
      run: () => st.updateSettings({ scene: id }),
    })),
  ];
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  const matched = actions.filter((a) => (words.length ? words.every((w) => a.label.toLowerCase().includes(w)) : a.group !== "Ambience"));
  const foodItems: Item[] = foods.map((f) => ({
    id: `f-${f.id}`,
    group: `Add one serving to ${MEAL_LABEL[meal]}`,
    label: f.name,
    hint: `${int(f.kcal)} kcal`,
    run: async () => {
      const e = await api.addEntry({ ...f, day: st.day, meal, food_id: f.id, qty: 1, save_food: false });
      st.bump();
      st.notify(`Added ${f.name} to ${MEAL_LABEL[meal]}`, async () => {
        await api.deleteEntry(e.id);
        useStore.getState().bump();
      });
    },
  }));
  const items = [...foodItems, ...matched];

  useEffect(() => {
    setSel(0);
  }, [q, foods.length]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-i="${sel}"]`)?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const run = async (it: Item) => {
    close();
    await it.run();
  };

  let lastGroup = "";
  return (
    <div className="anim-fade fixed inset-0 z-50 flex items-start justify-center pt-[14vh]" style={{ background: "rgb(10 8 6 / 0.28)" }} onMouseDown={close}>
      <div className="anim-pop w-[580px] overflow-hidden rounded-[18px]" style={{ background: "var(--surface-solid)", boxShadow: "var(--shadow-pop)" }} onMouseDown={(e) => e.stopPropagation()}>
        <div className="flex h-16 items-center gap-3 px-6" style={{ boxShadow: "0 1px 0 var(--line)" }}>
          <ISearch size={15} className="text-ink3" />
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Search foods, screens, ambience…"
            className="input-bare text-[17px]"
            onKeyDown={(e) => {
              if (e.key === "Escape") close();
              if (e.key === "ArrowDown") e.preventDefault(), setSel((i) => Math.min(i + 1, items.length - 1));
              if (e.key === "ArrowUp") e.preventDefault(), setSel((i) => Math.max(i - 1, 0));
              if (e.key === "Enter" && items[sel]) e.preventDefault(), void run(items[sel]);
            }}
          />
        </div>
        <div ref={listRef} className="scroll max-h-[380px] px-3 py-2">
          {items.map((it, i) => {
            const head = it.group !== lastGroup ? ((lastGroup = it.group), it.group) : null;
            return (
              <div key={it.id}>
                {head && <div className="label px-3 pb-1.5 pt-3">{head}</div>}
                <button
                  data-i={i}
                  onMouseMove={() => setSel(i)}
                  onClick={() => void run(it)}
                  className="flex h-10 w-full items-center gap-3 rounded-[10px] px-3 text-left text-[13.5px]"
                  style={{ background: sel === i ? "var(--sunk)" : undefined }}
                >
                  <span className="flex w-4 justify-center text-ink3">
                    {it.id.startsWith("f-") ? <INutrition size={13} /> : it.id === "water" ? <IDrop size={13} /> : it.id.startsWith("scene-") ? <Swatch id={it.id.slice(6) as SceneId} size={12} /> : null}
                  </span>
                  <span className="flex-1 truncate">{it.label}</span>
                  {it.hint && <span className="mono text-[10.5px] text-ink3">{it.hint}</span>}
                </button>
              </div>
            );
          })}
          {items.length === 0 && <p className="px-3 py-8 text-center text-[13px] text-ink3">No matches</p>}
        </div>
      </div>
    </div>
  );
}

// ———————————————————————————————————————————————————————————— toast

function Toast() {
  const toast = useStore((s) => s.toast);
  if (!toast) return null;
  return (
    <div
      key={toast.id}
      className="fixed bottom-7 left-1/2 z-[60] flex items-center gap-3 rounded-full py-2 pl-5 pr-2 text-[13px]"
      style={{ animation: "toast 300ms var(--ease) both", background: "var(--ink)", color: "var(--surface-solid)", boxShadow: "var(--shadow-pop)", marginLeft: 114 }}
    >
      {toast.text}
      {toast.undo ? (
        <button
          className="mono rounded-full px-3 py-1 text-[10.5px] font-medium uppercase tracking-[0.06em]"
          style={{ background: "color-mix(in srgb, var(--surface-solid) 16%, transparent)" }}
          onClick={async () => {
            useStore.setState({ toast: null });
            await toast.undo!();
          }}
        >
          Undo
        </button>
      ) : (
        <span className="w-2" />
      )}
    </div>
  );
}

// ———————————————————————————————————————————————————————————— first run

function Welcome() {
  const s = useSettings();
  const update = useStore((x) => x.updateSettings);
  const [name, setName] = useState(s.name);
  const [units, setUnits] = useState(s.units);
  const [weight, setWeight] = useState<number | null>(null);

  const finish = async (toProgram: boolean) => {
    if (weight != null) {
      const d = await api.getDay(today());
      await api.setMetrics(today(), { ...d.metrics, weight_kg: toKg(weight, units) });
    }
    await update({ name: name.trim(), units, onboarded: true });
    useStore.getState().bump();
    if (toProgram) useStore.getState().go("program");
  };

  return (
    <div className="anim-fade fixed inset-0 z-50 flex items-center justify-center" style={{ background: "rgb(10 8 6 / 0.3)" }}>
      <div className="anim-pop w-[560px] rounded-[22px] p-10" style={{ background: "var(--surface-solid)", boxShadow: "var(--shadow-pop)" }}>
        <div className="flex items-center gap-2.5">
          <Logo />
          <span className="label">Tally · first run</span>
        </div>
        <h1 className="display mt-8 text-[40px] leading-[1.05]">
          Count what matters.
          <br />
          <span className="text-ink3">Skip the rest.</span>
        </h1>
        <p className="mt-4 max-w-[420px] text-[13.5px] leading-relaxed text-ink2">
          Food, training, sleep and weight in one place. Answer a few questions on Program and Tally will work out your calories, macros and a full training plan. Every
          number follows a rule you can read, and nothing leaves this Mac.
        </p>
        <div className="mt-8 grid grid-cols-[1fr_auto] gap-x-8 gap-y-6">
          <Field label="Your name">
            <input autoFocus className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="Optional" />
          </Field>
          <Field label="Units">
            <Segmented
              className="h-[34px] items-center"
              value={units}
              onChange={setUnits}
              options={[
                { value: "metric", label: "kg · km" },
                { value: "imperial", label: "lb · mi" },
              ]}
            />
          </Field>
          <Field label="Weight this morning" className="col-span-2">
            <NumberField value={weight} onChange={(v) => setWeight(v == null ? null : round(v, 1))} step={0.1} dp={1} suffix={weightUnit(units)} stepper={false} placeholder="Optional" />
          </Field>
        </div>
        <div className="mt-10 flex items-center justify-between">
          <button className="btn -ml-3" onClick={() => void finish(false)}>
            Just let me log
          </button>
          <button className="btn btn-accent h-10 px-5" onClick={() => void finish(true)}>
            Build my plan →
          </button>
        </div>
      </div>
    </div>
  );
}
