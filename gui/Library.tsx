import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

interface LibraryStatus {
  db_path: string;
  exists: boolean;
  credentials_saved: boolean;
  roots: string[];
  present: number;
  enriched: number;
  pending: number;
}

interface Credentials {
  dev_id: string;
  dev_password: string;
  user: string;
  password: string;
}

interface DiscoverStats {
  added: number;
  changed: number;
  unchanged: number;
  removed: number;
  restored: number;
  cancelled: boolean;
}

interface EnrichStats {
  enriched: number;
  missing: number;
  failed: number;
  cancelled: boolean;
}

interface BrowseHit {
  file_id: number;
  name: string;
  title: string;
  year: string | null;
  publisher: string | null;
  thumb_path: string | null;
  needs_enrich: boolean;
}

interface ProgressView {
  message: string;
  filePos?: number;
  fileLen?: number;
}

type ProgressEvent =
  | { kind: "phase"; step: number; total: number; message: string }
  | { kind: "files"; step: number; total: number; pos: number; len: number; message: string };

const SEARCH_LIMIT = 80;

type LibraryScreen = "platforms" | "browse" | "settings";

/** Platforms with a wired folder screen. More entries land here as each one is implemented. */
const PLATFORMS = [{ id: "snes", name: "Super Nintendo" }] as const;

export default function Library({
  active,
  exportBusy,
  onBusyChange,
}: {
  active: boolean;
  exportBusy: boolean;
  onBusyChange: (busy: boolean) => void;
}) {
  const [screen, setScreen] = useState<LibraryScreen>("platforms");
  const [status, setStatus] = useState<LibraryStatus | null>(null);
  const [creds, setCreds] = useState<Credentials>({
    dev_id: "",
    dev_password: "",
    user: "",
    password: "",
  });
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<BrowseHit[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ProgressView | null>(null);
  const [busy, setBusy] = useState(false);
  const acceptProgress = useRef(false);
  const runId = useRef(0);
  const searchRequest = useRef(0);

  useEffect(() => {
    onBusyChange(busy);
  }, [busy, onBusyChange]);

  useEffect(() => {
    let activeListen = true;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const unlistenFn = await listen<ProgressEvent>("plan-progress", (event) => {
        if (!activeListen || !acceptProgress.current) {
          return;
        }
        const payload = event.payload;
        if (payload.kind === "files") {
          setProgress({
            message: payload.message,
            filePos: payload.pos,
            fileLen: payload.len,
          });
          return;
        }
        setProgress({ message: payload.message });
      });
      if (!activeListen) {
        unlistenFn();
        return;
      }
      unlisten = unlistenFn;
    })();
    return () => {
      activeListen = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!active) {
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const [nextStatus, stored] = await Promise.all([
          invoke<LibraryStatus>("library_status"),
          invoke<Credentials>("library_credentials"),
        ]);
        if (cancelled) {
          return;
        }
        setStatus(nextStatus);
        setCreds(stored);
        setError(null);
      } catch (loadError) {
        if (!cancelled) {
          setError(String(loadError));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [active]);

  useEffect(() => {
    if (!active) {
      return;
    }
    const request = ++searchRequest.current;
    const handle = window.setTimeout(() => {
      void invoke<BrowseHit[]>("library_search", { query, limit: SEARCH_LIMIT })
        .then((rows) => {
          if (request === searchRequest.current) {
            setHits(rows);
          }
        })
        .catch((searchError: unknown) => {
          if (request === searchRequest.current) {
            setError(String(searchError));
          }
        });
    }, 200);
    return () => window.clearTimeout(handle);
  }, [active, query, status]);

  async function refresh() {
    const next = await invoke<LibraryStatus>("library_status");
    searchRequest.current += 1;
    setStatus(next);
  }

  async function pickRoot(platformName: string) {
    const selected = await open({
      directory: true,
      multiple: false,
      title: `${platformName} ROM folder`,
    });
    if (typeof selected !== "string") {
      return;
    }
    setError(null);
    try {
      const next = await invoke<LibraryStatus>("library_add_root", { path: selected });
      searchRequest.current += 1;
      setStatus(next);
      setNotice(null);
    } catch (addError) {
      setError(String(addError));
    }
  }

  async function removeRoot(path: string) {
    setError(null);
    try {
      const next = await invoke<LibraryStatus>("library_remove_root", { path });
      searchRequest.current += 1;
      setStatus(next);
    } catch (removeError) {
      setError(String(removeError));
    }
  }

  async function saveCredentials() {
    setError(null);
    try {
      await invoke("library_save_credentials", { input: creds });
      setNotice("ScreenScraper credentials saved.");
      await refresh();
    } catch (saveError) {
      setError(String(saveError));
    }
  }

  function beginRun(): number {
    runId.current += 1;
    acceptProgress.current = true;
    setBusy(true);
    setError(null);
    return runId.current;
  }

  function endRun() {
    acceptProgress.current = false;
    setBusy(false);
    setProgress(null);
  }

  async function scan() {
    const id = beginRun();
    setProgress({ message: "Scanning…" });
    try {
      const stats = await invoke<DiscoverStats>("library_scan", { runId: id });
      const stopped = stats.cancelled ? " Stopped before the remaining folders." : "";
      setNotice(
        `Scan: ${stats.added} added, ${stats.changed} changed, ${stats.unchanged} unchanged, ${stats.removed} removed, ${stats.restored} restored.${stopped}`,
      );
      await refresh();
    } catch (scanError) {
      setError(String(scanError));
      await refresh().catch(() => undefined);
    } finally {
      endRun();
    }
  }

  async function enrich() {
    const id = beginRun();
    setProgress({ message: "Starting ScreenScraper…" });
    try {
      const stats = await invoke<EnrichStats>("library_enrich", { runId: id });
      const stopped = stats.cancelled ? " Stopped before the remaining files." : "";
      setNotice(
        `Scrape: ${stats.enriched} matched, ${stats.missing} not found, ${stats.failed} failed.${stopped}`,
      );
      await refresh();
    } catch (enrichError) {
      setError(String(enrichError));
      await refresh().catch(() => undefined);
    } finally {
      endRun();
    }
  }

  async function cancel() {
    await invoke("cancel_background_work", { runId: runId.current });
  }

  async function deleteIndex() {
    const confirmed = window.confirm(
      "Delete the library index and cached covers? ScreenScraper credentials stay saved.",
    );
    if (!confirmed) {
      return;
    }
    setError(null);
    searchRequest.current += 1;
    try {
      const next = await invoke<LibraryStatus>("library_delete");
      searchRequest.current += 1;
      setStatus(next);
      setHits([]);
      setNotice("Library index deleted.");
    } catch (deleteError) {
      setError(String(deleteError));
    }
  }

  async function openSearch(title: string) {
    try {
      await invoke("library_open_search", { title });
    } catch (openError) {
      setError(String(openError));
    }
  }

  const locked = busy || exportBusy;
  const filePercent =
    progress?.fileLen && progress.fileLen > 0 && progress.filePos !== undefined
      ? Math.min(100, Math.round((progress.filePos / progress.fileLen) * 100))
      : null;

  return (
    <div className="library">
      <nav className="screen-tabs" aria-label="Library">
        <button
          type="button"
          className={screen === "platforms" ? "active" : undefined}
          onClick={() => setScreen("platforms")}
        >
          Platforms
        </button>
        <button
          type="button"
          className={screen === "browse" ? "active" : undefined}
          onClick={() => setScreen("browse")}
        >
          Browse
        </button>
        <button
          type="button"
          className={screen === "settings" ? "active" : undefined}
          onClick={() => setScreen("settings")}
        >
          Settings
        </button>
      </nav>

      {progress && (
        <div className="progress-panel">
          <p className="progress">{progress.message}</p>
          {filePercent !== null && (
            <div
              className="progress-bar"
              role="progressbar"
              aria-valuenow={filePercent}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <div className="progress-bar-fill" style={{ width: `${filePercent}%` }} />
            </div>
          )}
          {progress.filePos !== undefined && progress.fileLen !== undefined && (
            <p className="hint">
              {progress.filePos.toLocaleString()} of {progress.fileLen.toLocaleString()}
            </p>
          )}
          {busy && (
            <button type="button" onClick={() => void cancel()}>
              Cancel
            </button>
          )}
        </div>
      )}
      {notice && <p className="hint">{notice}</p>}
      {error && <p className="error">{error}</p>}

      {screen === "platforms" && (
        <section className="panel">
          <h2 className="panel-title">ROM folders</h2>
          <p className="hint">
            Choose the folder for each platform before scraping. Scan reads those folders into the
            index. Browse scrapes titles and covers.
          </p>
          {PLATFORMS.map((platform) => {
            const wired = platform.id === "snes";
            return (
              <div key={platform.id} className="platform-block">
                <h3 className="panel-title">{platform.name}</h3>
                <p className="hint">
                  {wired && status
                    ? `${status.present.toLocaleString()} files indexed, ${status.pending.toLocaleString()} waiting to scrape.`
                    : wired
                      ? "Loading the library…"
                      : "Not available yet."}
                </p>
                {wired && status && status.roots.length > 0 ? (
                  <ul className="root-list">
                    {status.roots.map((path) => (
                      <li key={path}>
                        <span>{path}</span>
                        <button
                          type="button"
                          onClick={() => void removeRoot(path)}
                          disabled={locked}
                        >
                          Remove
                        </button>
                      </li>
                    ))}
                  </ul>
                ) : (
                  wired && <p className="hint">No folder yet.</p>
                )}
                <div className="actions">
                  <button
                    type="button"
                    onClick={() => void pickRoot(platform.name)}
                    disabled={locked || !wired}
                  >
                    Add folder
                  </button>
                  <button
                    type="button"
                    onClick={() => void scan()}
                    disabled={locked || !wired || !status || status.roots.length === 0}
                  >
                    Scan
                  </button>
                </div>
              </div>
            );
          })}
        </section>
      )}

      {screen === "browse" && (
        <section className="panel">
          <h2 className="panel-title">Games</h2>
          <p className="hint">
            {status
              ? `${status.enriched.toLocaleString()} with details, ${status.pending.toLocaleString()} waiting to scrape.`
              : "Loading the library…"}
          </p>
          <div className="actions">
            <button
              type="button"
              className="primary"
              onClick={() => void enrich()}
              disabled={locked || !status || status.pending === 0}
            >
              Scrape
            </button>
          </div>
          <label>
            Search
            <input
              value={query}
              onChange={(event) => setQuery(event.currentTarget.value)}
              placeholder="Title, year, publisher, or file name"
            />
          </label>
          <p className="hint">
            {hits.length === SEARCH_LIMIT
              ? `Showing the first ${SEARCH_LIMIT}. Type to narrow the list.`
              : `${hits.length.toLocaleString()} shown.`}
          </p>
          <div className="game-grid">
            {hits.map((hit) => (
              <article key={hit.file_id} className="game-card">
                {hit.thumb_path ? (
                  <img src={convertFileSrc(hit.thumb_path)} alt="" />
                ) : (
                  <div className="thumb-fallback">
                    {hit.needs_enrich ? "Not looked up" : "No cover"}
                  </div>
                )}
                <div className="game-card-body">
                  <strong>{hit.title}</strong>
                  <p>{[hit.year, hit.publisher].filter(Boolean).join(" · ") || hit.name}</p>
                  <button type="button" onClick={() => void openSearch(hit.title)}>
                    Google
                  </button>
                </div>
              </article>
            ))}
          </div>
        </section>
      )}

      {screen === "settings" && (
        <section className="panel">
          <h2 className="panel-title">ScreenScraper</h2>
          <p className="hint">
            {status?.credentials_saved
              ? "Saved in the app data folder. Scraping uses these before environment variables."
              : "Save a developer id and member login, or set the ROMS_CURATOR_SCREENSCRAPER_* variables."}
          </p>
          <div className="two-col">
            <label>
              Developer id
              <input
                value={creds.dev_id}
                onChange={(event) => setCreds({ ...creds, dev_id: event.currentTarget.value })}
                disabled={locked}
                autoComplete="off"
              />
            </label>
            <label>
              Developer password
              <input
                type="password"
                value={creds.dev_password}
                onChange={(event) =>
                  setCreds({ ...creds, dev_password: event.currentTarget.value })
                }
                disabled={locked}
                autoComplete="off"
              />
            </label>
            <label>
              User
              <input
                value={creds.user}
                onChange={(event) => setCreds({ ...creds, user: event.currentTarget.value })}
                disabled={locked}
                autoComplete="username"
              />
            </label>
            <label>
              Password
              <input
                type="password"
                value={creds.password}
                onChange={(event) => setCreds({ ...creds, password: event.currentTarget.value })}
                disabled={locked}
                autoComplete="current-password"
              />
            </label>
          </div>
          <div className="actions">
            <button type="button" onClick={() => void saveCredentials()} disabled={locked}>
              Save credentials
            </button>
          </div>
          <h2 className="panel-title">Index</h2>
          <p className="hint">
            {status
              ? `${status.db_path}${status.exists ? "" : " (not created yet)"}`
              : "Loading the library…"}
          </p>
          <div className="actions">
            <button type="button" onClick={() => void deleteIndex()} disabled={locked || !status?.exists}>
              Delete index
            </button>
          </div>
        </section>
      )}
    </div>
  );
}
