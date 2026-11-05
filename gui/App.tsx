import { useEffect, useMemo, useRef, useState } from "react";
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
  const [metadataRefresh, setMetadataRefresh] = useState(false);
  const [gameQuery, setGameQuery] = useState("");
  const [supportQuery, setSupportQuery] = useState("");
  const [progress, setProgress] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [review, setReview] = useState<PlanReviewResult | null>(null);
  const [exportResult, setExportResult] = useState<ExportResult | null>(null);
  const acceptProgress = useRef(false);
  const runId = useRef(0);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;

    void (async () => {
      const unlistenFn = await listen<string>("plan-progress", (event) => {
        if (active && acceptProgress.current) {
          setProgress(event.payload);
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

  useEffect(() => {
    setReview(null);
    setExportResult(null);
  }, [
    sourcePath,
    destinationPath,
    mameVersion,
    strictEmulation,
    subsetStart,
    subsetEnd,
    reportPath,
    metadataRefresh,
  ]);

  const exportInput = (): ExportInput => ({
    source_path: sourcePath,
    destination_path: destinationPath,
    mame_version: mameVersion,
    strict_emulation: strictEmulation,
    subset_start: subsetStart.trim() || undefined,
    subset_end: subsetEnd.trim() || undefined,
    report_path: reportPath.trim() || undefined,
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

  async function pickSource() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Merged ROM collection folder",
    });
    if (typeof selected === "string") {
      setSourcePath(selected);
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
    }
  }

  async function pickReportPath() {
    const selected = await save({
      title: "Export report (Markdown)",
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (typeof selected === "string") {
      setReportPath(selected);
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
    setProgress("Starting…");
    const id = beginRun();
    try {
      const result = await invoke<PlanReviewResult>("review_export_plan", {
        input: exportInput(),
        runId: id,
      });
      endRun();
      setReview(result);
      setProgress("Plan ready — review the lists below before exporting.");
    } catch (err) {
      endRun();
      const message = String(err);
      if (isCancelled(message)) {
        setProgress("Cancelled.");
      } else {
        setError(message);
        setProgress("");
      }
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
    setProgress("Starting export…");
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
      setProgress(result.all_ok ? "Export finished." : "Export finished with errors.");
    } catch (err) {
      endRun();
      const message = String(err);
      if (isCancelled(message)) {
        setReview(null);
        setProgress(
          "Cancelled. Files already copied stay in the destination. Preview again before exporting to this folder.",
        );
      } else {
        setError(message);
        setProgress("");
      }
    } finally {
      setBusy(false);
    }
  }

  const canReview =
    !busy && sourcePath && destinationPath && mameVersion.trim().length > 0;
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
          Pick your ROM folder and where to save a working set, preview what will
          be kept, then export for your emulator or frontend.
        </p>
      </header>

      <section className="panel">
        <h2 className="panel-title">Paths</h2>
        <label>
          MAME set version
          <input
            value={mameVersion}
            onChange={(e) => setMameVersion(e.currentTarget.value)}
            placeholder="0.289"
            disabled={busy}
          />
        </label>

        <label>
          Source collection
          <div className="path-row">
            <input
              value={sourcePath}
              onChange={(e) => setSourcePath(e.currentTarget.value)}
              placeholder="/path/to/merged/roms"
              disabled={busy}
            />
            <button type="button" onClick={pickSource} disabled={busy}>
              Browse…
            </button>
          </div>
        </label>

        <label>
          Export destination
          <div className="path-row">
            <input
              value={destinationPath}
              onChange={(e) => setDestinationPath(e.currentTarget.value)}
              placeholder="/path/to/export"
              disabled={busy}
            />
            <button type="button" onClick={pickDestination} disabled={busy}>
              Browse…
            </button>
          </div>
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
            Subset start (optional)
            <input
              value={subsetStart}
              onChange={(e) => setSubsetStart(e.currentTarget.value)}
              disabled={busy}
            />
          </label>
          <label>
            Subset end (optional)
            <input
              value={subsetEnd}
              onChange={(e) => setSubsetEnd(e.currentTarget.value)}
              disabled={busy}
            />
          </label>
        </div>
        <label>
          Report path (optional, must be outside source and export folders)
          <div className="path-row">
            <input
              value={reportPath}
              onChange={(e) => setReportPath(e.currentTarget.value)}
              placeholder="/path/to/export-report.md"
              disabled={busy}
            />
            <button type="button" onClick={pickReportPath} disabled={busy}>
              Browse…
            </button>
          </div>
        </label>
        <p className="hint">
          {review?.selection_help ??
            "Arcade games with minor MAME issues stay in the list by default. Support files needed by your selection are copied into the roms folder."}
        </p>
      </section>

      <section className="panel actions">
        <button
          type="button"
          className="primary"
          onClick={reviewPlan}
          disabled={!canReview}
        >
          {busy && !review ? "Building plan…" : "Preview plan"}
        </button>
        <button type="button" onClick={runExport} disabled={!canExport}>
          {busy && review ? "Exporting…" : "Export"}
        </button>
        {busy && (
          <button type="button" onClick={cancelWork}>
            Cancel
          </button>
        )}
        {exportSkipsMissing && (
          <p className="hint actions-hint">
            Some files are missing or unresolved in the plan. Export still runs;
            those files are skipped and listed in the report if you set a report
            path.
          </p>
        )}
      </section>

      {progress && <p className="progress">{progress}</p>}
      {error && <p className="error">{error}</p>}

      {review && (
        <>
          <section className="panel results">
            <h2>Plan summary</h2>
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
                <dt>Selected games</dt>
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
                <dt>Unresolved references</dt>
                <dd>{review.summary.unresolved}</dd>
              </div>
            </dl>
            {review.summary.missing_media > 0 && (
              <p className="hint">
                {review.summary.missing_media} planned file
                {review.summary.missing_media === 1 ? "" : "s"} not found in your
                source; export skips them.
              </p>
            )}
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

          <section className="panel">
            <h2>Selected games</h2>
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
            <h2>Support files for roms</h2>
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
                  <span>
                    {entry.role} · {entry.availability}
                  </span>
                  <p>{entry.reasons.join("; ")}</p>
                </li>
              ))}
            </ul>
          </section>

          {review.plan.unresolved.length > 0 && (
            <section className="panel">
              <h2>Unresolved references</h2>
              <ul className="scroll-list">
                {review.plan.unresolved.map((entry, index) => (
                  <li key={`${index}:${entry.game}:${entry.name}`}>
                    <strong>{entry.name}</strong>
                    <span>
                      {entry.kind} · game {entry.game || "(cycle)"}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </>
      )}

      {exportResult && (
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
      )}
    </main>
  );
}

export default App;
