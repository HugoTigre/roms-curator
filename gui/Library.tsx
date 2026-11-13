import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

interface PlatformStatus {
  id: string;
  name: string;
  uses_mame_version: boolean;
  mame_version: string | null;
  roots: string[];
  present: number;
  enriched: number;
  pending: number;
}

interface LibraryStatus {
  db_path: string;
  exists: boolean;
  credentials_saved: boolean;
  platforms: PlatformStatus[];
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
  playable: boolean | null;
  link: string | null;
  condition: "working" | "not_working" | "bios" | "system" | null;
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

type ScreenNote = { screen: LibraryScreen; text: string };

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
  const [browsePlatform, setBrowsePlatform] = useState("snes");
  const [versionDrafts, setVersionDrafts] = useState<Record<string, string>>({});
  const [hits, setHits] = useState<BrowseHit[]>([]);
  const [notice, setNotice] = useState<ScreenNote | null>(null);
  const [error, setError] = useState<ScreenNote | null>(null);
  const [progress, setProgress] = useState<ProgressView | null>(null);
  const [busy, setBusy] = useState(false);
  const acceptProgress = useRef(false);
  const runId = useRef(0);
  const searchRequest = useRef(0);
  const screenRef = useRef(screen);
  screenRef.current = screen;

  function clearError(where: LibraryScreen) {
    setError((current) => (current?.screen === where ? null : current));
  }

  function showError(where: LibraryScreen, value: unknown) {
    setError({ screen: where, text: String(value) });
  }

  function showNotice(where: LibraryScreen, text: string) {
    setNotice({ screen: where, text });
  }

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
        setVersionDrafts(versionDraftsFrom(nextStatus));
      } catch (loadError) {
        if (!cancelled) {
          showError(screenRef.current, loadError);
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
      void invoke<BrowseHit[]>("library_search", {
        platform: browsePlatform,
        query,
        limit: SEARCH_LIMIT,
      })
        .then((rows) => {
          if (request === searchRequest.current) {
            setHits(rows);
          }
        })
        .catch((searchError: unknown) => {
          if (request === searchRequest.current) {
            showError("browse", searchError);
          }
        });
    }, 200);
    return () => window.clearTimeout(handle);
  }, [active, browsePlatform, query, status]);

  async function refresh() {
    const next = await invoke<LibraryStatus>("library_status");
    searchRequest.current += 1;
    setStatus(next);
  }

  async function pickRoot(platformId: string, platformName: string) {
    const selected = await open({
      directory: true,
      multiple: false,
      title: `${platformName} ROM folder`,
    });
    if (typeof selected !== "string") {
      return;
    }
    clearError("platforms");
    try {
      const next = await invoke<LibraryStatus>("library_add_root", {
        platform: platformId,
        path: selected,
      });
      searchRequest.current += 1;
      setStatus(next);
      setNotice((current) => (current?.screen === "platforms" ? null : current));
    } catch (addError) {
      showError("platforms", addError);
    }
  }

  async function removeRoot(platformId: string, path: string) {
    clearError("platforms");
    try {
      const next = await invoke<LibraryStatus>("library_remove_root", {
        platform: platformId,
        path,
      });
      searchRequest.current += 1;
      setStatus(next);
    } catch (removeError) {
      showError("platforms", removeError);
    }
  }

  async function saveCredentials() {
    clearError("settings");
    try {
      await invoke("library_save_credentials", { input: creds });
      showNotice("settings", "ScreenScraper credentials saved.");
      await refresh();
    } catch (saveError) {
      showError("settings", saveError);
    }
  }

  function beginRun(where: LibraryScreen): number {
    runId.current += 1;
    acceptProgress.current = true;
    setBusy(true);
    clearError(where);
    return runId.current;
  }

  function endRun() {
    acceptProgress.current = false;
    setBusy(false);
    setProgress(null);
  }

  async function saveMameVersion(platformId: string, version: string) {
    clearError("platforms");
    const next = await invoke<LibraryStatus>("library_set_mame_version", {
      platform: platformId,
      version,
    });
    setStatus(next);
    setVersionDrafts(versionDraftsFrom(next));
    return next;
  }

  async function scan(platformId: string) {
    const id = beginRun("platforms");
    setProgress({ message: "Scanning…" });
    try {
      const stats = await invoke<DiscoverStats>("library_scan", {
        platform: platformId,
        runId: id,
      });
      const stopped = stats.cancelled ? " Stopped before the remaining folders." : "";
      showNotice(
        "platforms",
        `Scan: ${stats.added} added, ${stats.changed} changed, ${stats.unchanged} unchanged, ${stats.removed} removed, ${stats.restored} restored.${stopped}`,
      );
      await refresh();
    } catch (scanError) {
      showError("platforms", scanError);
      await refresh().catch(() => undefined);
    } finally {
      endRun();
    }
  }

  async function enrich() {
    const platform = status?.platforms.find((item) => item.id === browsePlatform);
    if (platform?.uses_mame_version) {
      const version = (versionDrafts[platform.id] ?? "").trim();
      if (!version) {
        showError("browse", `Set the MAME version for ${platform.name} before scraping.`);
        return;
      }
      try {
        await saveMameVersion(platform.id, version);
      } catch (saveError) {
        showError("browse", saveError);
        return;
      }
    }
    const id = beginRun("browse");
    setProgress({ message: "Scraping…" });
    try {
      const stats = await invoke<EnrichStats>("library_enrich", {
        platform: browsePlatform,
        runId: id,
      });
      const stopped = stats.cancelled ? " Stopped before the remaining files." : "";
      showNotice(
        "browse",
        `Scrape: ${stats.enriched} matched, ${stats.missing} not found, ${stats.failed} failed.${stopped}`,
      );
      await refresh();
    } catch (enrichError) {
      showError("browse", enrichError);
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
    clearError("settings");
    searchRequest.current += 1;
    try {
      const next = await invoke<LibraryStatus>("library_delete");
      searchRequest.current += 1;
      setStatus(next);
      setVersionDrafts(versionDraftsFrom(next));
      setHits([]);
      showNotice("settings", "Library index deleted.");
    } catch (deleteError) {
      showError("settings", deleteError);
    }
  }

  async function openSearch(title: string) {
    try {
      await invoke("library_open_search", { platform: browsePlatform, title });
    } catch (openError) {
      showError("browse", openError);
    }
  }

  async function openLink(url: string) {
    try {
      await invoke("library_open_link", { url });
    } catch (openError) {
      showError("browse", openError);
    }
  }

  const locked = busy || exportBusy;
  const browsed = status?.platforms.find((item) => item.id === browsePlatform) ?? null;
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
      {notice?.screen === screen && <p className="hint">{notice.text}</p>}
      {error?.screen === screen && <p className="error">{error.text}</p>}

      {screen === "platforms" && (
        <section className="panel">
          <h2 className="panel-title">ROM folders</h2>
          <p className="hint">
            Choose the folder for each platform before scraping. Scan reads those folders into the
            index. Browse fills in titles.
          </p>
          {!status && <p className="hint">Loading the library…</p>}
          {(status?.platforms ?? []).map((platform) => (
              <div key={platform.id} className="platform-block">
                <h3 className="panel-title">{platform.name}</h3>
                <p className="hint">
                  {status
                    ? `${platform.present.toLocaleString()} files indexed, ${platform.pending.toLocaleString()} waiting to scrape.`
                    : "Loading the library…"}
                </p>
                {platform.uses_mame_version && (
                  <label>
                    MAME version
                    <input
                      value={versionDrafts[platform.id] ?? ""}
                      onChange={(event) =>
                        setVersionDrafts({
                          ...versionDrafts,
                          [platform.id]: event.currentTarget.value,
                        })
                      }
                      placeholder="0.289"
                      disabled={locked}
                    />
                  </label>
                )}
                {platform.roots.length > 0 ? (
                  <ul className="root-list">
                    {platform.roots.map((path) => (
                      <li key={path}>
                        <span>{path}</span>
                        <button
                          type="button"
                          onClick={() => void removeRoot(platform.id, path)}
                          disabled={locked}
                        >
                          Remove
                        </button>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="hint">No folder yet.</p>
                )}
                <div className="actions">
                  {platform.uses_mame_version && (
                    <button
                      type="button"
                      onClick={() =>
                        void saveMameVersion(platform.id, versionDrafts[platform.id] ?? "").then(
                          () => {
                            clearError("browse");
                            showNotice("platforms", "MAME version saved.");
                          },
                          (saveError: unknown) => showError("platforms", saveError),
                        )
                      }
                      disabled={locked}
                    >
                      Save version
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => void pickRoot(platform.id, platform.name)}
                    disabled={locked}
                  >
                    Add folder
                  </button>
                  <button
                    type="button"
                    onClick={() => void scan(platform.id)}
                    disabled={locked || platform.roots.length === 0}
                  >
                    Scan
                  </button>
                </div>
              </div>
            ))}
        </section>
      )}

      {screen === "browse" && (
        <section className="panel">
          <h2 className="panel-title">Games</h2>
          <label>
            Platform
            <select
              value={browsePlatform}
              onChange={(event) => {
                searchRequest.current += 1;
                setHits([]);
                setBrowsePlatform(event.currentTarget.value);
              }}
              disabled={locked}
            >
              {(status?.platforms ?? [{ id: "snes", name: "Super Nintendo" }]).map((platform) => (
                <option key={platform.id} value={platform.id}>
                  {platform.name}
                </option>
              ))}
            </select>
          </label>
          <p className="hint">
            {browsed
              ? `${browsed.enriched.toLocaleString()} with details, ${browsed.pending.toLocaleString()} waiting to scrape.`
              : "Loading the library…"}
          </p>
          <div className="actions">
            <button
              type="button"
              className="primary"
              onClick={() => void enrich()}
              disabled={locked || !browsed || browsed.pending === 0}
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
                  <p>
                    {[hit.year, hit.publisher, conditionLabel(hit)]
                      .filter(Boolean)
                      .join(" · ") || hit.name}
                  </p>
                  <button type="button" onClick={() => void openSearch(hit.title)}>
                    Google
                  </button>
                  {hit.link && (
                    <button type="button" onClick={() => void openLink(hit.link ?? "")}>
                      Arcade Database
                    </button>
                  )}
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
              : "Dev Username and Dev Password are the API login. Member and Password are the login for the site."}
          </p>
          <p className="hint">
            Leave the debug password out. ScreenScraper limits that password to 100 uses a day.
          </p>
          <div className="two-col">
            <label>
              Dev Username
              <input
                value={creds.dev_id}
                onChange={(event) => setCreds({ ...creds, dev_id: event.currentTarget.value })}
                disabled={locked}
                autoComplete="off"
              />
            </label>
            <label>
              Dev Password
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
              Member
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

function versionDraftsFrom(status: LibraryStatus): Record<string, string> {
  return Object.fromEntries(
    status.platforms.map((platform) => [platform.id, platform.mame_version ?? ""]),
  );
}

function conditionLabel(hit: BrowseHit): string | null {
  switch (hit.condition) {
    case "working":
      return "Working";
    case "not_working":
      return "Not working";
    case "bios":
      return "BIOS";
    case "system":
      return "System";
    default:
      if (hit.playable === true) {
        return "Working";
      }
      if (hit.playable === false) {
        return "Not working";
      }
      return null;
  }
}
