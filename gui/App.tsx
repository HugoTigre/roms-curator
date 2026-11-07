import {
  useEffect,
  useMemo,
  useRef,
  useState,
  useTransition,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./App.css";

interface PlanPreviewSummary {
  mame_build: string;
  emulation_policy: string;
  selected_games: number;
  present_media: number;
  missing_media: number;
  unresolved: number;
  missing_chds: number;
  excluded_games: number;
  conflicts: string[];
}

interface PlannedGame {
  machine: string;
  category: string;
  folder: string;
  reason: string;
}

interface PlannedMedia {
  machine: string;
  role: "archive" | "disk";
  availability: "present" | "missing";
  source?: string | null;
  destination?: string | null;
  bytes?: number | null;
  reasons: string[];
}

interface UnresolvedMedia {
  game: string;
  kind: string;
  name: string;
}

interface PlanDocument {
  version: number;
  metadata: Record<string, string>;
  games: PlannedGame[];
  media: PlannedMedia[];
  unresolved: UnresolvedMedia[];
  conflicts: string[];
  [key: string]: unknown;
}

interface PlanReviewResult {
  summary: PlanPreviewSummary;
  selection_help: string;
  plan: PlanDocument;
  missing_chds: MissingChdEntry[];
  excluded_games: PlannedGame[];
}

interface MissingChdEntry {
  game: string;
  machine: string;
  disk: string;
  reason: string;
}

type Screen = "setup" | "review" | "export";

function countLabel(total: number, shown?: number): string {
  if (shown !== undefined && shown !== total) {
    return ` (${shown.toLocaleString()} of ${total.toLocaleString()})`;
  }
  return ` (${total.toLocaleString()})`;
}

const PLAN_SUMMARY_HELP =
  "High-level counts for this preview: selected games, support files found or missing in the source, CHD gaps, dependency warnings, and machines left off the list. Review the sections below before copying files.";

const GAMES_SECTION_HELP =
  "Standard selection keeps arcade games MAME rates good or imperfect. Strict keeps only fully working games.";

const UNRESOLVED_SECTION_HELP =
  "Dependency checks from MAME metadata. Disk gaps usually also appear under Missing CHDs; this list also covers missing parent ROM zips and broken dependency chains. Selected/working will still be copied in export (unless there is a destination conflict). A game with a gap here may copy to roms/ but will not run in MAME until the missing file is added.";

const SUPPORT_FILES_HELP =
  "This list is what would land in roms/ because at least one working game needs it.";

const EXPORT_STAYS_SELECTED =
  "The game stays on your selected list; export still copies its ROM zip and any CHDs you already have.";

function formatSupportRole(role: PlannedMedia["role"]): string {
  return role === "disk" ? "CHD folder" : "ROM zip";
}

function unresolvedDescription(entry: UnresolvedMedia): string {
  const game = entry.game ? `“${entry.game}”` : "a selected game";
  let detail: string;
  switch (entry.kind) {
    case "delta-parent":
      detail = `${game} uses a delta CHD: file “${entry.name}” must already exist as a full CHD in your source (the delta base). That base was not found.`;
      break;
    case "own-disk":
    case "shared-disk":
    case "device-disk":
      detail = `${game} needs disk “${entry.name}”, but no matching CHD was found in the source.`;
      break;
    case "parent-archive":
    case "rom-parent-archive":
    case "own-archive":
    case "device-archive":
      detail = `${game} needs ROM zip “${entry.name}” (${entry.kind.replace(/-/g, " ")}), but that zip is not in the source.`;
      break;
    case "dependency-cycle":
    case "disk-cycle":
      detail = `Circular reference in ${entry.kind.replace(/-/g, " ")}: ${entry.name}`;
      break;
    default:
      detail = `${game} — ${entry.kind}: ${entry.name}`;
  }
  return `${detail} ${EXPORT_STAYS_SELECTED}`;
}

/** Must stay in sync with `roms_curator::REST_MEDIA_REASON`. */
const REST_MEDIA_REASON = "Not required by a selected game";

function supportMediaFromPlan(media: PlannedMedia[]): PlannedMedia[] {
  return media.filter((entry) =>
    entry.reasons.some((reason) => reason !== REST_MEDIA_REASON),
  );
}

interface ExportInput {
  source_path: string;
  destination_path: string;
  mame_version: string;
  strict_emulation: boolean;
  subset_start?: string;
  subset_end?: string;
  report_path?: string;
  plan_html_path?: string;
  metadata_refresh: boolean;
}

interface ExportResult {
  all_ok: boolean;
  selected_games: number;
  roms_entries: number;
  rest_entries: number;
  failed_copies: number;
  missing_chds: number;
  report_path: string;
  report_written: boolean;
  names_error?: string | null;
}

interface WritePlanReportResult {
  report_path: string;
  report_written: boolean;
}

interface WritePlanHtmlResult {
  plan_html_path: string;
  plan_html_written: boolean;
}

interface FormIssues {
  mame_version: string | null;
  source_path: string | null;
  destination_path: string | null;
  report_path: string | null;
  plan_html_path: string | null;
  subset_start: string | null;
  subset_end: string | null;
}

function shownIssue(
  issue: string | null | undefined,
  value: string,
  touched: boolean,
): string | null {
  if (!issue) {
    return null;
  }
  if (!value.trim() && !touched) {
    return null;
  }
  return issue;
}

function matchesQuery(text: string, query: string): boolean {
  return text.toLowerCase().includes(query.trim().toLowerCase());
}

function formatEmulationPolicy(policy: string): string {
  switch (policy) {
    case "playable":
      return "Standard";
    case "strict":
      return "Strict";
    default:
      return policy;
  }
}

interface ProgressPayload {
  kind: "phase";
  step: number;
  total: number;
  message: string;
}

interface FileProgressPayload {
  kind: "files";
  step: number;
  total: number;
  pos: number;
  len: number;
  message: string;
}

type PlanProgressEvent = ProgressPayload | FileProgressPayload;

interface ProgressView {
  step: number;
  total: number;
  message: string;
  filePos?: number;
  fileLen?: number;
}

function progressFromEvent(event: PlanProgressEvent): ProgressView {
  if (event.kind === "files") {
    return {
      step: event.step,
      total: event.total,
      message: event.message,
      filePos: event.pos,
      fileLen: event.len,
    };
  }
  return {
    step: event.step,
    total: event.total,
    message: event.message,
  };
}

function progressPercent(view: ProgressView): number | null {
  if (view.total <= 0) {
    return null;
  }
  if (
    view.fileLen !== undefined &&
    view.filePos !== undefined &&
    view.fileLen > 0
  ) {
    const completedPhases = Math.max(0, view.step - 1);
    const inStep = view.filePos / view.fileLen;
    return Math.min(
      100,
      Math.round(((completedPhases + inStep) / view.total) * 100),
    );
  }
  return Math.min(100, Math.round((view.step / view.total) * 100));
}

function isCancelled(message: string): boolean {
  return message.toLowerCase().includes("cancelled");
}

function App() {
  const [sourcePath, setSourcePath] = useState("");
  const [destinationPath, setDestinationPath] = useState("");
  const [mameVersion, setMameVersion] = useState("0.289");
  const [strictEmulation, setStrictEmulation] = useState(false);
  const [subsetStart, setSubsetStart] = useState("");
  const [subsetEnd, setSubsetEnd] = useState("");
  const [reportPath, setReportPath] = useState("");
  const [planHtmlPath, setPlanHtmlPath] = useState("");
  const [metadataRefresh, setMetadataRefresh] = useState(false);
  const [gameQuery, setGameQuery] = useState("");
  const [supportQuery, setSupportQuery] = useState("");
  const [unresolvedQuery, setUnresolvedQuery] = useState("");
  const [missingChdQuery, setMissingChdQuery] = useState("");
  const [excludedQuery, setExcludedQuery] = useState("");
  const [screen, setScreen] = useState<Screen>("setup");
  const [reviewListsReady, setReviewListsReady] = useState(false);
  const [isReviewPending, startReviewTransition] = useTransition();
  const [planExportNotes, setPlanExportNotes] = useState<string[]>([]);
  const [progress, setProgress] = useState<ProgressView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [review, setReview] = useState<PlanReviewResult | null>(null);
  const [exportResult, setExportResult] = useState<ExportResult | null>(null);
  const [issues, setIssues] = useState<FormIssues | null>(null);
  const [formCheckError, setFormCheckError] = useState<string | null>(null);
  const [touchedPaths, setTouchedPaths] = useState({
    source: false,
    destination: false,
    report: false,
    plan_html: false,
  });
  const acceptProgress = useRef(false);
  const runId = useRef(0);
  const formCheck = useRef(0);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;

    void (async () => {
      const unlistenFn = await listen<PlanProgressEvent>("plan-progress", (event) => {
        if (active && acceptProgress.current) {
          setProgress(progressFromEvent(event.payload));
        }
      });
      if (!active) {
        unlistenFn();
        return;
      }
      unlisten = unlistenFn;
    })();

    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  function goToReview() {
    if (!review) {
      return;
    }
    setReviewListsReady(false);
    setPlanExportNotes([]);
    startReviewTransition(() => setScreen("review"));
  }

  useEffect(() => {
    if (screen !== "review" || !review) {
      setReviewListsReady(false);
      return;
    }
    const timeout = window.setTimeout(() => setReviewListsReady(true), 0);
    return () => window.clearTimeout(timeout);
  }, [screen, review]);

  useEffect(() => {
    setReview(null);
    setExportResult(null);
    setPlanExportNotes([]);
    setScreen("setup");
  }, [
    sourcePath,
    destinationPath,
    mameVersion,
    strictEmulation,
    subsetStart,
    subsetEnd,
    reportPath,
    planHtmlPath,
    metadataRefresh,
  ]);

  useEffect(() => {
    const request = ++formCheck.current;
    const handle = window.setTimeout(() => {
      void invoke<FormIssues>("validate_export_form", {
        input: {
          source_path: sourcePath,
          destination_path: destinationPath,
          mame_version: mameVersion,
          subset_start: subsetStart.trim() || undefined,
          subset_end: subsetEnd.trim() || undefined,
          report_path: reportPath.trim() || undefined,
          plan_html_path: planHtmlPath.trim() || undefined,
        },
      })
        .then((next) => {
          if (formCheck.current === request) {
            setFormCheckError(null);
            setIssues(next);
          }
        })
        .catch(() => {
          if (formCheck.current === request) {
            setIssues(null);
            setFormCheckError("Could not check these fields. Try editing one and checking again.");
          }
        });
    }, 200);
    return () => window.clearTimeout(handle);
  }, [sourcePath, destinationPath, mameVersion, subsetStart, subsetEnd, reportPath, planHtmlPath]);

  const exportInput = (): ExportInput => ({
    source_path: sourcePath,
    destination_path: destinationPath,
    mame_version: mameVersion,
    strict_emulation: strictEmulation,
    subset_start: subsetStart.trim().toLowerCase() || undefined,
    subset_end: subsetEnd.trim().toLowerCase() || undefined,
    report_path: reportPath.trim() || undefined,
    plan_html_path: planHtmlPath.trim() || undefined,
    metadata_refresh: metadataRefresh,
  });

  const filteredGames = useMemo(() => {
    if (!review) {
      return [];
    }
    const games = review.plan.games;
    if (!gameQuery.trim()) {
      return games;
    }
    return games.filter(
      (game) =>
        matchesQuery(game.machine, gameQuery) ||
        matchesQuery(game.category, gameQuery),
    );
  }, [review, gameQuery]);

  const filteredSupport = useMemo(() => {
    if (!review) {
      return [];
    }
    const support = supportMediaFromPlan(review.plan.media);
    if (!supportQuery.trim()) {
      return support;
    }
    return support.filter(
      (entry) =>
        matchesQuery(entry.machine, supportQuery) ||
        entry.reasons.some((reason) => matchesQuery(reason, supportQuery)),
    );
  }, [review, supportQuery]);

  const filteredUnresolved = useMemo(() => {
    if (!review) {
      return [];
    }
    const rows = review.plan.unresolved;
    if (!unresolvedQuery.trim()) {
      return rows;
    }
    return rows.filter(
      (entry) =>
        matchesQuery(entry.name, unresolvedQuery) ||
        matchesQuery(entry.kind, unresolvedQuery) ||
        matchesQuery(entry.game, unresolvedQuery) ||
        matchesQuery(unresolvedDescription(entry), unresolvedQuery),
    );
  }, [review, unresolvedQuery]);

  const filteredMissingChds = useMemo(() => {
    if (!review) {
      return [];
    }
    const rows = review.missing_chds;
    if (!missingChdQuery.trim()) {
      return rows;
    }
    return rows.filter(
      (entry) =>
        matchesQuery(entry.game, missingChdQuery) ||
        matchesQuery(entry.machine, missingChdQuery) ||
        matchesQuery(entry.disk, missingChdQuery) ||
        matchesQuery(entry.reason, missingChdQuery),
    );
  }, [review, missingChdQuery]);

  const filteredExcluded = useMemo(() => {
    if (!review) {
      return [];
    }
    const rows = review.excluded_games;
    if (!excludedQuery.trim()) {
      return rows;
    }
    return rows.filter(
      (game) =>
        matchesQuery(game.machine, excludedQuery) ||
        matchesQuery(game.category, excludedQuery) ||
        matchesQuery(game.reason, excludedQuery),
    );
  }, [review, excludedQuery]);

  async function pickSource() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Merged ROM collection folder",
    });
    if (typeof selected === "string") {
      setSourcePath(selected);
      setTouchedPaths((current) => ({ ...current, source: true }));
    }
  }

  async function pickDestination() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Export destination folder",
    });
    if (typeof selected === "string") {
      setDestinationPath(selected);
      setTouchedPaths((current) => ({ ...current, destination: true }));
    }
  }

  async function pickReportPath() {
    const selected = await save({
      title: "Export report (Markdown)",
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (typeof selected === "string") {
      setReportPath(selected);
      setTouchedPaths((current) => ({ ...current, report: true }));
    }
  }

  async function pickPlanHtmlPath() {
    const selected = await save({
      title: "Plan preview (HTML)",
      filters: [{ name: "HTML", extensions: ["html"] }],
    });
    if (typeof selected === "string") {
      setPlanHtmlPath(selected);
      setTouchedPaths((current) => ({ ...current, plan_html: true }));
    }
  }

  async function cancelWork() {
    await invoke("cancel_background_work", { runId: runId.current });
  }

  function beginRun(): number {
    runId.current += 1;
    acceptProgress.current = true;
    return runId.current;
  }

  function endRun() {
    acceptProgress.current = false;
  }

  async function reviewPlan() {
    setBusy(true);
    setError(null);
    setReview(null);
    setExportResult(null);
    setProgress({ step: 0, total: 10, message: "Starting…" });
    const id = beginRun();
    try {
      const result = await invoke<PlanReviewResult>("review_export_plan", {
        input: exportInput(),
        runId: id,
      });
      endRun();
      setReview(result);
      setReviewListsReady(false);
      startReviewTransition(() => setScreen("review"));
      setProgress({
        step: 10,
        total: 10,
        message: "Plan ready — review the lists below before exporting.",
      });
    } catch (err) {
      endRun();
      const message = String(err);
      if (isCancelled(message)) {
        setProgress({ step: 0, total: 10, message: "Cancelled." });
      } else {
        setError(message);
        setProgress(null);
      }
    } finally {
      setBusy(false);
    }
  }

  function planPreviewRequest() {
    if (!review) {
      return null;
    }
    return {
      input: exportInput(),
      plan: review.plan,
      missing_chds: review.missing_chds,
      excluded_games: review.excluded_games,
    };
  }

  async function writePlanReport() {
    const request = planPreviewRequest();
    if (!request) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<WritePlanReportResult>("write_plan_report", { request });
      if (result.report_written) {
        setPlanExportNotes((current) => [
          ...current,
          `Markdown plan report: ${result.report_path}`,
        ]);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function writePlanHtml() {
    const request = planPreviewRequest();
    if (!request) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<WritePlanHtmlResult>("write_plan_html", { request });
      if (result.plan_html_written) {
        setPlanExportNotes((current) => [
          ...current,
          `HTML plan preview: ${result.plan_html_path}`,
        ]);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function runExport() {
    if (!review) {
      return;
    }
    setBusy(true);
    setError(null);
    setExportResult(null);
    setProgress({ step: 0, total: 9, message: "Starting export…" });
    const id = beginRun();
    try {
      const result = await invoke<ExportResult>("execute_export", {
        runId: id,
        request: {
          input: exportInput(),
          plan: review.plan,
        },
      });
      endRun();
      setExportResult(result);
      setScreen("export");
      setProgress({
        step: 9,
        total: 9,
        message: result.all_ok ? "Export finished." : "Export finished with errors.",
      });
    } catch (err) {
      endRun();
      const message = String(err);
      if (isCancelled(message)) {
        setReview(null);
        setScreen("setup");
        setProgress({
          step: 0,
          total: 9,
          message:
            "Cancelled. Files already copied stay in the destination. Preview again before exporting to this folder.",
        });
      } else {
        setError(message);
        setProgress(null);
      }
    } finally {
      setBusy(false);
    }
  }

  const versionIssue = issues?.mame_version ?? null;
  const sourceIssue = shownIssue(issues?.source_path, sourcePath, touchedPaths.source);
  const destinationIssue = shownIssue(
    issues?.destination_path,
    destinationPath,
    touchedPaths.destination,
  );
  const reportIssue = shownIssue(issues?.report_path, reportPath, touchedPaths.report);
  const planHtmlIssue = shownIssue(
    issues?.plan_html_path,
    planHtmlPath,
    touchedPaths.plan_html,
  );
  const subsetStartIssue = issues?.subset_start ?? null;
  const subsetEndIssue = issues?.subset_end ?? null;
  const formReady =
    !!issues &&
    !issues.mame_version &&
    !issues.source_path &&
    !issues.destination_path &&
    !issues.report_path &&
    !issues.plan_html_path &&
    !issues.subset_start &&
    !issues.subset_end;

  const canReview = !busy && formReady;
  const canExport =
    !!review &&
    !busy &&
    review.summary.conflicts.length === 0 &&
    review.summary.selected_games > 0;
  const exportSkipsMissing =
    !!review &&
    review.summary.conflicts.length === 0 &&
    review.summary.selected_games > 0 &&
    (review.summary.missing_media > 0 || review.summary.unresolved > 0);

  return (
    <main className="app">
      <header>
        <h1>Roms Curator</h1>
        <p className="lede">
          Set paths and preferences, preview the plan, then export when you are ready.
        </p>
      </header>

      <nav className="screen-tabs" aria-label="Workflow">
        <button
          type="button"
          className={screen === "setup" ? "active" : undefined}
          onClick={() => setScreen("setup")}
        >
          1. Setup
        </button>
        <button
          type="button"
          className={screen === "review" ? "active" : undefined}
          onClick={goToReview}
          disabled={!review || isReviewPending}
        >
          2. Plan review
        </button>
        <button
          type="button"
          className={screen === "export" ? "active" : undefined}
          onClick={() => exportResult && setScreen("export")}
          disabled={!exportResult}
        >
          3. Export result
        </button>
      </nav>

      {screen === "setup" && (
        <>
      <section className="panel">
        <h2 className="panel-title">Paths</h2>
        <label>
          MAME set version
          <input
            value={mameVersion}
            onChange={(e) => setMameVersion(e.currentTarget.value)}
            placeholder="0.289"
            disabled={busy}
            aria-invalid={versionIssue ? true : undefined}
            className={versionIssue ? "invalid" : undefined}
          />
          {versionIssue ? (
            <p className="field-error">{versionIssue}</p>
          ) : (
            <p className="hint">Version of your ROM set, such as 0.289.</p>
          )}
        </label>

        <label>
          Source collection
          <div className="path-row">
            <input
              value={sourcePath}
              onChange={(e) => setSourcePath(e.currentTarget.value)}
              onBlur={() => setTouchedPaths((current) => ({ ...current, source: true }))}
              placeholder="/path/to/merged/roms"
              disabled={busy}
              aria-invalid={sourceIssue ? true : undefined}
              className={sourceIssue ? "invalid" : undefined}
            />
            <button type="button" onClick={pickSource} disabled={busy}>
              Browse…
            </button>
          </div>
          {sourceIssue && <p className="field-error">{sourceIssue}</p>}
        </label>

        <label>
          Export destination
          <div className="path-row">
            <input
              value={destinationPath}
              onChange={(e) => setDestinationPath(e.currentTarget.value)}
              onBlur={() =>
                setTouchedPaths((current) => ({ ...current, destination: true }))
              }
              placeholder="/path/to/export"
              disabled={busy}
              aria-invalid={destinationIssue ? true : undefined}
              className={destinationIssue ? "invalid" : undefined}
            />
            <button type="button" onClick={pickDestination} disabled={busy}>
              Browse…
            </button>
          </div>
          {destinationIssue && <p className="field-error">{destinationIssue}</p>}
        </label>
      </section>

      <section className="panel">
        <h2 className="panel-title">Preferences</h2>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={strictEmulation}
            onChange={(e) => setStrictEmulation(e.currentTarget.checked)}
            disabled={busy}
          />
          Strict selection (only fully working games)
        </label>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={metadataRefresh}
            onChange={(e) => setMetadataRefresh(e.currentTarget.checked)}
            disabled={busy}
          />
          Re-download listxml and CatVer for this MAME version
        </label>
        <div className="two-col">
          <label>
            From (optional)
            <input
              value={subsetStart}
              onChange={(e) => setSubsetStart(e.currentTarget.value)}
              placeholder="0"
              disabled={busy}
              aria-invalid={subsetStartIssue ? true : undefined}
              className={subsetStartIssue ? "invalid" : undefined}
            />
            {subsetStartIssue && <p className="field-error">{subsetStartIssue}</p>}
          </label>
          <label>
            Through (optional)
            <input
              value={subsetEnd}
              onChange={(e) => setSubsetEnd(e.currentTarget.value)}
              placeholder="zzzzzzzzzzzzzzzz"
              disabled={busy}
              aria-invalid={subsetEndIssue ? true : undefined}
              className={subsetEndIssue ? "invalid" : undefined}
            />
            {subsetEndIssue && <p className="field-error">{subsetEndIssue}</p>}
          </label>
        </div>
        <p className="hint">
          Leave both empty to include every name. The gray 0 and z's are examples, not a filter.
          A value is a point in alphabetical order, and a full name is allowed.
          From `galaga` keeps `galaga` and later names such as `gblaga`, and skips earlier
          names such as 1942. Through those z's includes `zaxxon` and the rest of the list.
          Parent, BIOS, and disk files that a working game needs are still copied.
        </p>
        <label>
          Report path (optional, must be outside source and export folders)
          <div className="path-row">
            <input
              value={reportPath}
              onChange={(e) => setReportPath(e.currentTarget.value)}
              onBlur={() => setTouchedPaths((current) => ({ ...current, report: true }))}
              placeholder="/path/to/export-report.md"
              disabled={busy}
              aria-invalid={reportIssue ? true : undefined}
              className={reportIssue ? "invalid" : undefined}
            />
            <button type="button" onClick={pickReportPath} disabled={busy}>
              Browse…
            </button>
          </div>
          {reportIssue && <p className="field-error">{reportIssue}</p>}
        </label>
        <label>
          Plan HTML path (optional, for collapsible browser preview)
          <div className="path-row">
            <input
              value={planHtmlPath}
              onChange={(e) => setPlanHtmlPath(e.currentTarget.value)}
              onBlur={() => setTouchedPaths((current) => ({ ...current, plan_html: true }))}
              placeholder="/path/to/plan-preview.html"
              disabled={busy}
              aria-invalid={planHtmlIssue ? true : undefined}
              className={planHtmlIssue ? "invalid" : undefined}
            />
            <button type="button" onClick={pickPlanHtmlPath} disabled={busy}>
              Browse…
            </button>
          </div>
          {planHtmlIssue && <p className="field-error">{planHtmlIssue}</p>}
        </label>
        <p className="hint">
          Arcade games with minor MAME issues stay on the list by default. Support
          files needed by your selection are copied into the roms folder.
        </p>
      </section>

      <section className="panel actions actions-setup">
        {review && !busy && (
          <button type="button" onClick={goToReview} disabled={isReviewPending}>
            Open plan review
          </button>
        )}
        {busy && (
          <button type="button" onClick={cancelWork}>
            Cancel
          </button>
        )}
        <button
          type="button"
          className="primary"
          onClick={reviewPlan}
          disabled={!canReview}
        >
          {busy ? "Building plan…" : "Preview plan"}
        </button>
      </section>

      {progress && (busy || screen === "setup") && (
        <div className="progress-panel" aria-live="polite">
          <p className="progress">
            Step {progress.step}/{progress.total} — {progress.message}
            {progress.fileLen !== undefined &&
              progress.filePos !== undefined &&
              progress.fileLen > 0 && (
                <span className="progress-files">
                  {" "}
                  ({progress.filePos.toLocaleString()}/
                  {progress.fileLen.toLocaleString()} source entries)
                </span>
              )}
          </p>
          {progressPercent(progress) !== null && (
            <div
              className="progress-bar"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={progressPercent(progress) ?? 0}
            >
              <div
                className="progress-bar-fill"
                style={{ width: `${progressPercent(progress)}%` }}
              />
            </div>
          )}
        </div>
      )}
        </>
      )}

      {formCheckError && <p className="error">{formCheckError}</p>}
      {error && <p className="error">{error}</p>}

      {screen === "review" && review && (
        <>
          {(isReviewPending || !reviewListsReady) && (
            <p className="review-opening" role="status" aria-live="polite">
              Opening plan review… loading lists (
              {review.summary.selected_games.toLocaleString()} games and support
              files). This can take a few seconds.
            </p>
          )}

          <section className="panel results">
            <h2>Plan summary</h2>
            <p className="hint">{PLAN_SUMMARY_HELP}</p>
            <dl>
              <div>
                <dt>MAME build</dt>
                <dd>{review.summary.mame_build}</dd>
              </div>
              <div>
                <dt>Selection</dt>
                <dd>{formatEmulationPolicy(review.summary.emulation_policy)}</dd>
              </div>
              <div>
                <dt>Selected / working games</dt>
                <dd>{review.summary.selected_games}</dd>
              </div>
              <div>
                <dt>Media present</dt>
                <dd>{review.summary.present_media}</dd>
              </div>
              <div>
                <dt>Media missing</dt>
                <dd>{review.summary.missing_media}</dd>
              </div>
              <div>
                <dt>Missing CHDs</dt>
                <dd>{review.summary.missing_chds}</dd>
              </div>
              <div>
                <dt>Unresolved references</dt>
                <dd>{review.summary.unresolved}</dd>
              </div>
              <div>
                <dt>Not on working list</dt>
                <dd>{review.summary.excluded_games}</dd>
              </div>
            </dl>
            {review.summary.missing_media > 0 && (
              <p className="hint">
                {review.summary.missing_media} planned file
                {review.summary.missing_media === 1 ? "" : "s"} not found in your
                source; export skips them.
              </p>
            )}
            {exportSkipsMissing && (
              <p className="hint">
                Some files are missing or unresolved in the plan. Export still runs;
                those items are skipped and appear in the report when a report path
                is set.
              </p>
            )}
            <p className="hint">
              <strong>Write plan report</strong> and <strong>Write HTML preview</strong> (below)
              save this preview without copying files. Markdown suits diffing and tooling; HTML
              opens in a browser with collapsible sections. Set paths on Setup first.
            </p>
            {planExportNotes.map((note) => (
              <p key={note} className="hint">
                {note}
              </p>
            ))}
            {review.summary.conflicts.length > 0 && (
              <div className="conflicts">
                <h3>Destination conflicts</h3>
                <p>Resolve these before export.</p>
                <ul>
                  {review.summary.conflicts.map((line, index) => (
                    <li key={`${index}:${line}`}>{line}</li>
                  ))}
                </ul>
              </div>
            )}
          </section>

          {reviewListsReady && !isReviewPending && (
            <>
          <section className="panel">
            <h2>
              Selected / working games
              {countLabel(review.plan.games.length, filteredGames.length)}
            </h2>
            <p className="hint">{GAMES_SECTION_HELP}</p>
            <input
              className="search"
              value={gameQuery}
              onChange={(e) => setGameQuery(e.currentTarget.value)}
              placeholder="Filter by name or category…"
            />
            <ul className="scroll-list">
              {filteredGames.map((game) => (
                <li key={game.machine}>
                  <strong>{game.machine}</strong>
                  <span>{game.category}</span>
                  <p>{game.reason}</p>
                </li>
              ))}
            </ul>
          </section>

          <section className="panel">
            <h2>
              Support files for roms
              {countLabel(
                supportMediaFromPlan(review.plan.media).length,
                filteredSupport.length,
              )}
            </h2>
            <p className="hint">{SUPPORT_FILES_HELP}</p>
            <input
              className="search"
              value={supportQuery}
              onChange={(e) => setSupportQuery(e.currentTarget.value)}
              placeholder="Filter by machine or reason…"
            />
            <ul className="scroll-list">
              {filteredSupport.map((entry, index) => (
                <li key={`${index}:${entry.machine}:${entry.role}`}>
                  <strong>{entry.machine}</strong>
                  <span>{formatSupportRole(entry.role)}</span>
                  {entry.availability === "missing" && (
                    <p className="support-missing">Not in your source — export skips this file</p>
                  )}
                  <p>{entry.reasons.join("; ")}</p>
                </li>
              ))}
            </ul>
          </section>

          {review.missing_chds.length > 0 && (
            <section className="panel">
              <h2>
                Missing CHDs
                {countLabel(review.missing_chds.length, filteredMissingChds.length)}
              </h2>
              <p className="hint">
                CHD files selected/working games need that are not in your source.
                Same list as the export report.
              </p>
              <input
                className="search"
                value={missingChdQuery}
                onChange={(e) => setMissingChdQuery(e.currentTarget.value)}
                placeholder="Filter by game, machine folder, or disk…"
              />
              <ul className="scroll-list">
                {filteredMissingChds.map((entry, index) => (
                  <li key={`${index}:${entry.game}:${entry.disk}`}>
                    <strong>{entry.disk}</strong>
                    <span>
                      game {entry.game} · folder {entry.machine}
                    </span>
                    <p>{entry.reason}</p>
                  </li>
                ))}
              </ul>
            </section>
          )}

          {review.plan.unresolved.length > 0 && (
            <section className="panel">
              <h2>
                Unresolved references
                {countLabel(review.plan.unresolved.length, filteredUnresolved.length)}
              </h2>
              <p className="hint">{UNRESOLVED_SECTION_HELP}</p>
              <input
                className="search"
                value={unresolvedQuery}
                onChange={(e) => setUnresolvedQuery(e.currentTarget.value)}
                placeholder="Filter by name, kind, or game…"
              />
              <ul className="scroll-list">
                {filteredUnresolved.map((entry, index) => (
                  <li key={`${index}:${entry.game}:${entry.name}:${entry.kind}`}>
                    <strong>{entry.name}</strong>
                    <span>{entry.kind}</span>
                    <p>{unresolvedDescription(entry)}</p>
                  </li>
                ))}
              </ul>
            </section>
          )}

          {review.excluded_games.length > 0 && (
            <section className="panel">
              <h2>
                Not on working list
                {countLabel(review.excluded_games.length, filteredExcluded.length)}
              </h2>
              <p className="hint">
                Arcade-style machines kept off the selected list (preliminary,
                non-arcade CatVer groups, strict policy, mechanical, and similar).
              </p>
              <input
                className="search"
                value={excludedQuery}
                onChange={(e) => setExcludedQuery(e.currentTarget.value)}
                placeholder="Filter by name, category, or reason…"
              />
              <ul className="scroll-list">
                {filteredExcluded.map((game) => (
                  <li key={game.machine}>
                    <strong>{game.machine}</strong>
                    <span>{game.category}</span>
                    <p>{game.reason}</p>
                  </li>
                ))}
              </ul>
            </section>
          )}
            </>
          )}

          {progress && busy && (
            <div className="progress-panel" aria-live="polite">
              <p className="progress">
                Step {progress.step}/{progress.total} — {progress.message}
                {progress.fileLen !== undefined &&
                  progress.filePos !== undefined &&
                  progress.fileLen > 0 && (
                    <span className="progress-files">
                      {" "}
                      ({progress.filePos.toLocaleString()}/
                      {progress.fileLen.toLocaleString()} source entries)
                    </span>
                  )}
              </p>
              {progressPercent(progress) !== null && (
                <div
                  className="progress-bar"
                  role="progressbar"
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={progressPercent(progress) ?? 0}
                >
                  <div
                    className="progress-bar-fill"
                    style={{ width: `${progressPercent(progress)}%` }}
                  />
                </div>
              )}
            </div>
          )}

          <section className="panel actions actions-review">
            <div className="actions-leading">
              <button type="button" onClick={() => setScreen("setup")} disabled={busy}>
                Back to setup
              </button>
              <button
                type="button"
                onClick={writePlanReport}
                disabled={busy || !reportPath.trim() || !!reportIssue}
                title={
                  reportPath.trim()
                    ? undefined
                    : "Set a Markdown report path on Setup first."
                }
              >
                Write plan report
              </button>
              <button
                type="button"
                onClick={writePlanHtml}
                disabled={busy || !planHtmlPath.trim() || !!planHtmlIssue}
                title={
                  planHtmlPath.trim()
                    ? undefined
                    : "Set a plan HTML path on Setup first."
                }
              >
                Write HTML preview
              </button>
              {busy && (
                <button type="button" onClick={cancelWork}>
                  Cancel
                </button>
              )}
            </div>
            <button
              type="button"
              className="primary export-action"
              onClick={runExport}
              disabled={!canExport}
            >
              {busy ? "Exporting…" : "Export"}
            </button>
          </section>
        </>
      )}

      {screen === "export" && exportResult && (
        <>
        <section className={`panel results ${exportResult.all_ok ? "ok" : "warn"}`}>
          <h2>Export result</h2>
          <dl>
            <div>
              <dt>Status</dt>
              <dd>{exportResult.all_ok ? "All OK" : "Completed with errors"}</dd>
            </div>
            <div>
              <dt>Selected games</dt>
              <dd>{exportResult.selected_games}</dd>
            </div>
            <div>
              <dt>Roms folder entries</dt>
              <dd>{exportResult.roms_entries}</dd>
            </div>
            <div>
              <dt>Rest folder entries</dt>
              <dd>{exportResult.rest_entries}</dd>
            </div>
            <div>
              <dt>Failed copies</dt>
              <dd>{exportResult.failed_copies}</dd>
            </div>
            <div>
              <dt>Missing CHDs</dt>
              <dd>{exportResult.missing_chds}</dd>
            </div>
          </dl>
          {exportResult.report_written && (
            <p className="hint">Report written to {exportResult.report_path}</p>
          )}
          {exportResult.names_error && (
            <p className="error">{exportResult.names_error}</p>
          )}
        </section>
        <section className="panel actions">
          <button type="button" onClick={() => setScreen("review")}>
            Back to plan review
          </button>
          <button type="button" onClick={() => setScreen("setup")}>
            Back to setup
          </button>
        </section>
        </>
      )}
    </main>
  );
}

export default App;
