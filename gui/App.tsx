import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

export interface PlanPreviewResult {
  mame_build: string;
  emulation_policy: string;
  selected_games: number;
  present_media: number;
  missing_media: number;
  unresolved: number;
  conflicts: string[];
}

function App() {
  const [sourcePath, setSourcePath] = useState("");
  const [destinationPath, setDestinationPath] = useState("");
  const [mameVersion, setMameVersion] = useState("0.289");
  const [progress, setProgress] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<PlanPreviewResult | null>(null);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;

    void (async () => {
      const unlistenFn = await listen<string>("plan-progress", (event) => {
        if (active) {
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

  async function buildPlanPreview() {
    setBusy(true);
    setError(null);
    setPreview(null);
    setProgress("Starting…");
    try {
      const result = await invoke<PlanPreviewResult>("build_plan_preview", {
        input: {
          source_path: sourcePath,
          destination_path: destinationPath,
          mame_version: mameVersion,
        },
      });
      setPreview(result);
      setProgress("Plan ready.");
    } catch (err) {
      setError(String(err));
      setProgress("");
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="app">
      <header>
        <h1>Roms Curator</h1>
        <p className="lede">
          Choose a merged source folder and export destination, then preview the
          copy plan before exporting.
        </p>
      </header>

      <section className="panel">
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

        <button
          type="button"
          className="primary"
          onClick={buildPlanPreview}
          disabled={
            busy ||
            !sourcePath ||
            !destinationPath ||
            !mameVersion.trim()
          }
        >
          {busy ? "Building plan…" : "Preview plan"}
        </button>
      </section>

      {progress && <p className="progress">{progress}</p>}
      {error && <p className="error">{error}</p>}

      {preview && (
        <section className="panel results">
          <h2>Plan preview</h2>
          <dl>
            <div>
              <dt>MAME build</dt>
              <dd>{preview.mame_build}</dd>
            </div>
            <div>
              <dt>Policy</dt>
              <dd>{preview.emulation_policy}</dd>
            </div>
            <div>
              <dt>Selected games</dt>
              <dd>{preview.selected_games}</dd>
            </div>
            <div>
              <dt>Media present</dt>
              <dd>{preview.present_media}</dd>
            </div>
            <div>
              <dt>Media missing</dt>
              <dd>{preview.missing_media}</dd>
            </div>
            <div>
              <dt>Unresolved references</dt>
              <dd>{preview.unresolved}</dd>
            </div>
          </dl>
          {preview.conflicts.length > 0 && (
            <div className="conflicts">
              <h3>Destination conflicts</h3>
              <ul>
                {preview.conflicts.map((line, index) => (
                  <li key={`${index}:${line}`}>{line}</li>
                ))}
              </ul>
            </div>
          )}
        </section>
      )}
    </main>
  );
}

export default App;
