import { useEffect, useLayoutEffect, useRef, useState } from "react";
import Menu from "./Menu";
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

interface UnmappedFile {
  file_id: number;
  name: string;
  relative_path: string;
}

interface UnmappedList {
  total: number;
  files: UnmappedFile[];
}

interface ScreenScraperQuota {
  max_per_minute: number | null;
  max_per_day: number | null;
  used_today: number | null;
  max_misses_per_day: number | null;
  misses_today: number | null;
  max_threads: number | null;
  max_download_kbps: number | null;
}

interface LibraryStatus {
  db_path: string;
  exists: boolean;
  credentials_saved: boolean;
  igdb_credentials_saved: boolean;
  download_covers: boolean;
  screenscraper: ScreenScraperQuota;
  platforms: PlatformStatus[];
}

interface Credentials {
  dev_id: string;
  dev_password: string;
  user: string;
  password: string;
}

interface IgdbCredentials {
  client_id: string;
  client_secret: string;
}

interface IgdbCandidate {
  id: number;
  name: string;
  year: string | null;
  category: string;
}

interface DiscoverStats {
  added: number;
  changed: number;
  unchanged: number;
  removed: number;
  restored: number;
  cancelled: boolean;
}

interface QuotaPause {
  games_left: number;
  fits: number;
  remaining_file_ids: number[];
}

interface EnrichStats {
  enriched: number;
  missing: number;
  failed: number;
  cancelled: boolean;
  quota_pause?: QuotaPause | null;
  visited_file_ids?: number[];
}

type QuotaChoice = "continue" | "limit" | "cancel";

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
  provider: string | null;
  provider_id: string | null;
}

interface BrowsePage {
  hits: BrowseHit[];
  total: number;
  offset: number;
}

interface ProgressView {
  message: string;
  filePos?: number;
  fileLen?: number;
}

type ProgressEvent =
  | { kind: "phase"; step: number; total: number; message: string }
  | { kind: "files"; step: number; total: number; pos: number; len: number; message: string };

const SEARCH_LIMIT = 100;
const LOG_LIMIT = 500;

export type LibraryScreen = "platforms" | "browse" | "settings" | "limits" | "unmapped" | "logs";

type BrowseView = "cards" | "list";

const BROWSE_PLATFORM_KEY = "roms-curator.browse-platform";
const BROWSE_VIEW_KEY = "roms-curator.browse-view";

function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStored(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // The choice still applies until this window closes.
  }
}

function storedBrowseView(): BrowseView {
  return readStored(BROWSE_VIEW_KEY) === "list" ? "list" : "cards";
}

type ScreenNote = { screen: LibraryScreen; text: string };

export default function Library({
  active,
  exportBusy,
  screen,
  onBusyChange,
}: {
  active: boolean;
  exportBusy: boolean;
  screen: LibraryScreen;
  onBusyChange: (busy: boolean) => void;
}) {
  const [status, setStatus] = useState<LibraryStatus | null>(null);
  const [creds, setCreds] = useState<Credentials>({
    dev_id: "",
    dev_password: "",
    user: "",
    password: "",
  });
  const [igdbCreds, setIgdbCreds] = useState<IgdbCredentials>({
    client_id: "",
    client_secret: "",
  });
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const [pageQuery, setPageQuery] = useState(0);
  const [total, setTotal] = useState(0);
  const [pageOffset, setPageOffset] = useState(0);
  const [browsePlatform, setBrowsePlatform] = useState(
    () => readStored(BROWSE_PLATFORM_KEY) || "snes",
  );
  const [browseView, setBrowseView] = useState<BrowseView>(storedBrowseView);
  const [detailId, setDetailId] = useState<number | null>(null);
  const [versionDrafts, setVersionDrafts] = useState<Record<string, string>>({});
  const [hits, setHits] = useState<BrowseHit[]>([]);
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  const [notice, setNotice] = useState<ScreenNote | null>(null);
  const [error, setError] = useState<ScreenNote | null>(null);
  const [progress, setProgress] = useState<ProgressView | null>(null);
  const [downloadCovers, setDownloadCovers] = useState(true);
  const [unmappedPlatform, setUnmappedPlatform] = useState("snes");
  const [unmapped, setUnmapped] = useState<UnmappedList | null>(null);
  const [unmappedSelected, setUnmappedSelected] = useState<number[]>([]);
  const [unmappedReload, setUnmappedReload] = useState(0);
  const [mapFile, setMapFile] = useState<UnmappedFile | null>(null);
  const [mapQuery, setMapQuery] = useState("");
  const [mapHits, setMapHits] = useState<IgdbCandidate[]>([]);
  const [mapNote, setMapNote] = useState<{ error: boolean; text: string } | null>(null);
  const [mapBusy, setMapBusy] = useState(false);
  const [logs, setLogs] = useState<string[]>([]);
  const [thumbRev, setThumbRev] = useState(0);
  const [busy, setBusy] = useState(false);
  const acceptProgress = useRef(false);
  const jobOpen = useRef(false);
  const runId = useRef(0);
  const searchRequest = useRef(0);
  const screenRef = useRef(screen);
  const logListRef = useRef<HTMLUListElement>(null);
  const mapDialogRef = useRef<HTMLDialogElement>(null);
  const quotaDialogRef = useRef<HTMLDialogElement>(null);
  const quotaChoice = useRef<((choice: QuotaChoice) => void) | null>(null);
  const [quotaPrompt, setQuotaPrompt] = useState<QuotaPause | null>(null);
  const unmappedSelectAllRef = useRef<HTMLInputElement>(null);
  const focusDetail = useRef(false);
  const followFileId = useRef<number | null>(null);
  const pageTurn = useRef<"start" | "end" | null>(null);
  const queryDelay = useRef("");
  const browseRef = useRef<HTMLDivElement>(null);
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
    writeStored(BROWSE_PLATFORM_KEY, browsePlatform);
  }, [browsePlatform]);

  useEffect(() => {
    writeStored(BROWSE_VIEW_KEY, browseView);
  }, [browseView]);

  useEffect(() => {
    if (!status || status.platforms.length === 0) {
      return;
    }
    if (!status.platforms.some((platform) => platform.id === browsePlatform)) {
      setBrowsePlatform(status.platforms[0].id);
    }
  }, [status, browsePlatform]);

  useEffect(() => {
    let activeListen = true;
    let unlisten: (() => void) | undefined;
    let unlistenLog: (() => void) | undefined;
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
      const unlistenLogFn = await listen<{ message: string }>("library-log", (event) => {
        const message = event.payload.message;
        setLogs((current) => {
          const next = current.length >= LOG_LIMIT ? current.slice(current.length - LOG_LIMIT + 1) : [...current];
          next.push(message);
          return next;
        });
      });
      if (!activeListen) {
        unlistenFn();
        unlistenLogFn();
        return;
      }
      unlisten = unlistenFn;
      unlistenLog = unlistenLogFn;
    })();
    return () => {
      activeListen = false;
      unlisten?.();
      unlistenLog?.();
    };
  }, []);

  useEffect(() => {
    const list = logListRef.current;
    if (screen === "logs" && list) {
      list.scrollTop = list.scrollHeight;
    }
  }, [logs, screen]);

  useEffect(() => {
    if (!active) {
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const [nextStatus, stored, igdbStored] = await Promise.all([
          invoke<LibraryStatus>("library_status"),
          invoke<Credentials>("library_credentials"),
          invoke<IgdbCredentials>("library_igdb_credentials"),
        ]);
        if (cancelled) {
          return;
        }
        setStatus(nextStatus);
        setDownloadCovers(nextStatus.download_covers);
        setCreds(stored);
        setIgdbCreds(igdbStored);
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
    if (!active || screen !== "unmapped") {
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const next = await invoke<UnmappedList>("library_unmapped", {
          platform: unmappedPlatform,
        });
        if (!cancelled) {
          setUnmapped(next);
          setMapFile((current) =>
            current && next.files.some((file) => file.file_id === current.file_id) ? current : null,
          );
        }
      } catch (loadError) {
        if (!cancelled) {
          showError("unmapped", loadError);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [active, screen, unmappedPlatform, unmappedReload]);

  useEffect(() => {
    if (!active) {
      return;
    }
    const request = ++searchRequest.current;
    const anchor = followFileId.current;
    followFileId.current = null;
    const delay = queryDelay.current === query ? 0 : 200;
    queryDelay.current = query;
    const handle = window.setTimeout(() => {
      void invoke<BrowsePage>("library_search", {
        platform: browsePlatform,
        query,
        limit: SEARCH_LIMIT,
        offset: pageQuery * SEARCH_LIMIT,
        anchor,
      })
        .then((result) => {
          if (request === searchRequest.current) {
            const rows = result.hits;
            setHits(rows);
            setTotal(result.total);
            setPageOffset(result.offset);
            const loaded = Math.floor(result.offset / SEARCH_LIMIT);
            setPage((current) => (current === loaded ? current : loaded));
            setPageQuery((current) => (current === loaded ? current : loaded));
            const visible = new Set(rows.map((row) => row.file_id));
            setSelectedIds((current) => current.filter((id) => visible.has(id)));
            if (anchor !== null && visible.has(anchor)) {
              pageTurn.current = null;
              focusDetail.current = true;
              setDetailId(anchor);
            } else if (pageTurn.current) {
              const edge = pageTurn.current;
              pageTurn.current = null;
              const target =
                edge === "end" ? rows[rows.length - 1]?.file_id : rows[0]?.file_id;
              if (target !== undefined) {
                focusDetail.current = true;
                setDetailId(target);
              }
            } else {
              setDetailId((current) =>
                current !== null && visible.has(current) ? current : (rows[0]?.file_id ?? null),
              );
            }
          }
        })
        .catch((searchError: unknown) => {
          if (request === searchRequest.current) {
            pageTurn.current = null;
            setPageQuery(page);
            showError("browse", searchError);
          }
        });
    }, delay);
    return () => window.clearTimeout(handle);
  }, [active, browsePlatform, query, status, pageQuery]);

  async function refresh() {
    const next = await invoke<LibraryStatus>("library_status");
    searchRequest.current += 1;
    setStatus(next);
    setDownloadCovers(next.download_covers);
  }

  function showPage(next: number, edge: "start" | "end" = "start") {
    const last = Math.max(0, Math.ceil(total / SEARCH_LIMIT) - 1);
    const clamped = Math.min(last, Math.max(0, next));
    if (clamped === pageQuery) return;
    pageTurn.current = edge;
    setPageQuery(clamped);
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

  function openMap(file: UnmappedFile) {
    setMapFile(file);
    setMapQuery(file.name.replace(/\.(zip|7z)$/i, ""));
    setMapHits([]);
    setMapNote(null);
  }

  function closeMap() {
    setMapFile(null);
    setMapHits([]);
    setMapNote(null);
  }

  async function searchIgdb() {
    const name = mapQuery.trim();
    if (!name) {
      setMapNote({ error: true, text: "Type a name to search." });
      return;
    }
    setMapNote(null);
    setMapBusy(true);
    try {
      const hits = await invoke<IgdbCandidate[]>("library_igdb_search", { query: name });
      setMapHits(hits);
      if (hits.length === 0) {
        setMapNote({ error: false, text: `IGDB has no results for ${name}.` });
      }
    } catch (searchError) {
      const text = String(searchError);
      setMapNote({ error: true, text });
      setLogs((current) => [...current, text].slice(-LOG_LIMIT));
    } finally {
      setMapBusy(false);
    }
  }

  async function assignIgdb(gameId: number) {
    if (!mapFile) {
      return;
    }
    setMapNote(null);
    setMapBusy(true);
    try {
      await invoke("library_igdb_assign", { fileId: mapFile.file_id, gameId });
      const mapped = `${mapFile.name} mapped.`;
      setLogs((current) => [...current, mapped].slice(-LOG_LIMIT));
      showNotice(screenRef.current, mapped);
      setUnmappedSelected((current) => current.filter((id) => id !== mapFile.file_id));
      setMapHits([]);
      setMapFile(null);
      if (screenRef.current === "browse") {
        followFileId.current = mapFile.file_id;
      }
      setThumbRev((current) => current + 1);
      setUnmappedReload((current) => current + 1);
      await refresh();
    } catch (assignError) {
      const text = String(assignError);
      setMapNote({ error: true, text });
      setLogs((current) => [...current, text].slice(-LOG_LIMIT));
    } finally {
      setMapBusy(false);
    }
  }

  async function saveIgdbCredentials() {
    clearError("settings");
    try {
      await invoke("library_save_igdb_credentials", { input: igdbCreds });
      showNotice("settings", "IGDB credentials saved.");
      await refresh();
    } catch (saveError) {
      showError("settings", saveError);
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

  function beginRun(where: LibraryScreen): number | null {
    if (jobOpen.current || exportBusy) {
      return null;
    }
    jobOpen.current = true;
    runId.current += 1;
    acceptProgress.current = true;
    setBusy(true);
    clearError(where);
    return runId.current;
  }

  function endRun() {
    jobOpen.current = false;
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
    if (id === null) {
      return;
    }
    setProgress({ message: "Scanning…" });
    try {
      const stats = await invoke<DiscoverStats>("library_scan", {
        platform: platformId,
        runId: id,
      });
      const stopped = stats.cancelled ? " Stopped before the remaining folders." : "";
      const summary = `Scan: ${stats.added} added, ${stats.changed} changed, ${stats.unchanged} unchanged, ${stats.removed} removed, ${stats.restored} restored.${stopped}`;
      showNotice("platforms", summary);
      await refresh();
    } catch (scanError) {
      showError("platforms", scanError);
      await refresh().catch(() => undefined);
    } finally {
      endRun();
    }
  }

  function toggleSelected(fileId: number) {
    setSelectedIds((current) =>
      current.includes(fileId) ? current.filter((id) => id !== fileId) : [...current, fileId],
    );
  }

  function askQuota(pause: QuotaPause): Promise<QuotaChoice> {
    return new Promise((resolve) => {
      quotaChoice.current = resolve;
      setQuotaPrompt(pause);
    });
  }

  function chooseQuota(choice: QuotaChoice) {
    quotaChoice.current?.(choice);
    quotaChoice.current = null;
    setQuotaPrompt(null);
  }

  async function invokeEnrich(
    run: number,
    platform: string,
    options: {
      fileIds?: number[];
      coverPass?: boolean;
      ignoreQuota?: boolean;
    },
  ): Promise<EnrichStats> {
    return invoke<EnrichStats>("library_enrich", {
      platform,
      runId: run,
      ...(options.fileIds && options.fileIds.length > 0 ? { fileIds: options.fileIds } : {}),
      ...(options.coverPass ? { coverPass: true } : {}),
      ...(options.ignoreQuota ? { ignoreQuota: true } : {}),
    });
  }

  async function enrichWithinQuota(
    run: number,
    platform: string,
    fileIds: number[] | undefined,
    coverPass: boolean,
  ): Promise<EnrichStats> {
    let stats = { enriched: 0, missing: 0, failed: 0, cancelled: false };
    let nextIds = fileIds;
    let ignoreQuota = false;
    for (;;) {
      const part = await invokeEnrich(run, platform, {
        fileIds: nextIds,
        coverPass,
        ignoreQuota,
      });
      stats = {
        enriched: stats.enriched + part.enriched,
        missing: stats.missing + part.missing,
        failed: stats.failed + part.failed,
        cancelled: stats.cancelled || part.cancelled,
      };
      const pause = part.quota_pause;
      if (!pause) {
        return stats;
      }
      setProgress({ message: "ScreenScraper limit reached." });
      const choice = await askQuota(pause);
      if (choice === "cancel" || (choice === "limit" && pause.fits <= 0)) {
        stats.cancelled = choice === "cancel";
        return stats;
      }
      ignoreQuota = true;
      nextIds =
        choice === "limit"
          ? pause.remaining_file_ids.slice(0, pause.fits)
          : pause.remaining_file_ids;
      setProgress({ message: coverPass ? "Scraping covers…" : "Scraping…" });
    }
  }

  async function enrich(
    fileIds?: number[],
    scope?: { screen: LibraryScreen; platform: string },
  ) {
    const where = scope?.screen ?? "browse";
    const platformId = scope?.platform ?? browsePlatform;
    if (fileIds && fileIds.length === 0) {
      const message = "Choose a game to scrape.";
      setLogs((current) => [...current, message].slice(-LOG_LIMIT));
      showError(where, message);
      return;
    }
    const id = beginRun(where);
    if (id === null) {
      return;
    }
    const rememberCursor = () => {
      if (where !== "browse") {
        return;
      }
      const followed =
        detailId !== null && (!fileIds || fileIds.includes(detailId))
          ? detailId
          : fileIds?.[0];
      if (followed !== undefined) {
        followFileId.current = followed;
      }
    };
    setProgress({ message: "Scraping…" });
    try {
      const platform = status?.platforms.find((item) => item.id === platformId);
      if (platform?.uses_mame_version) {
        const version = (versionDrafts[platform.id] ?? "").trim();
        if (!version) {
          const message = `Set the MAME version for ${platform.name} before scraping.`;
          setLogs((current) => [...current, message].slice(-LOG_LIMIT));
          showError(where, message);
          return;
        }
        await saveMameVersion(platform.id, version);
      }
      const finish = (summary: string) => {
        setLogs((current) => [...current, summary].slice(-LOG_LIMIT));
        showNotice(where, summary);
      };
      if (platformId === "arcade") {
        const text = await invokeEnrich(id, platformId, { fileIds, ignoreQuota: true });
        const textStopped = text.cancelled ? " Stopped before the remaining files." : "";
        const textSummary = `Scrape: ${text.enriched} matched, ${text.missing} not found, ${text.failed} failed.${textStopped}`;
        setLogs((current) => [...current, textSummary].slice(-LOG_LIMIT));
        showNotice(where, textSummary);
        const coverIds =
          fileIds && fileIds.length > 0 ? fileIds : (text.visited_file_ids ?? []);
        if (downloadCovers && coverIds.length > 0) {
          setProgress({ message: "Scraping covers…" });
          const covers = await enrichWithinQuota(id, platformId, coverIds, true);
          const stopped = covers.cancelled ? " Stopped before the remaining files." : "";
          finish(
            `Covers: ${covers.enriched} looked up, ${covers.missing} not found, ${covers.failed} failed.${stopped}`,
          );
        }
      } else {
        const stats = await enrichWithinQuota(id, platformId, fileIds, false);
        const stopped = stats.cancelled ? " Stopped before the remaining files." : "";
        finish(
          `Scrape: ${stats.enriched} matched, ${stats.missing} not found, ${stats.failed} failed.${stopped}`,
        );
      }
      setThumbRev((current) => current + 1);
      rememberCursor();
      await refresh();
      if (where === "unmapped") {
        setUnmappedSelected([]);
        setUnmappedReload((current) => current + 1);
      }
    } catch (enrichError) {
      showError(where, enrichError);
      rememberCursor();
      await refresh().catch(() => undefined);
    } finally {
      endRun();
    }
  }

  async function cancel() {
    await invoke("cancel_background_work", { runId: runId.current });
  }

  async function openDbFolder() {
    clearError("settings");
    try {
      await invoke("library_open_db_folder");
    } catch (openError) {
      showError("settings", openError);
    }
  }

  async function deleteIndex() {
    const confirmed = window.confirm(
      "Delete the library index and cached covers? ScreenScraper and IGDB credentials stay saved.",
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
      setSelectedIds([]);
      setDetailId(null);
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
  const unmappedShownIds = unmapped?.files.map((file) => file.file_id) ?? [];
  const allUnmappedSelected =
    unmappedShownIds.length > 0 &&
    unmappedShownIds.every((id) => unmappedSelected.includes(id));
  useEffect(() => {
    if (unmappedSelectAllRef.current) {
      unmappedSelectAllRef.current.indeterminate =
        unmappedSelected.length > 0 && !allUnmappedSelected;
    }
  }, [unmappedSelected, allUnmappedSelected]);
  const browsed = status?.platforms.find((item) => item.id === browsePlatform) ?? null;

  useLayoutEffect(() => {
    const root = browseRef.current;
    if (!root || detailId == null) return;
    const row = root.querySelector<HTMLElement>(`[data-file-id="${detailId}"]`);
    if (!row) return;
    row.scrollIntoView({ block: "nearest", inline: "nearest" });
    if (!focusDetail.current) return;
    focusDetail.current = false;
    row.querySelector<HTMLElement>("button.game-list-copy")?.focus({ preventScroll: true });
  }, [detailId, hits, browseView]);

  useEffect(() => {
    const dialog = quotaDialogRef.current;
    if (!quotaPrompt || !dialog) return;
    if (!dialog.open) dialog.showModal();
    return () => {
      if (dialog.open) dialog.close();
    };
  }, [quotaPrompt]);

  useEffect(() => {
    const dialog = mapDialogRef.current;
    if (!mapFile || !dialog) return;
    if (!dialog.open) dialog.showModal();
    dialog.querySelector("input")?.focus();
    return () => {
      if (dialog.open) dialog.close();
    };
  }, [mapFile]);

  const filePercent =
    progress?.fileLen && progress.fileLen > 0 && progress.filePos !== undefined
      ? Math.min(100, Math.round((progress.filePos / progress.fileLen) * 100))
      : null;

  return (
    <div className="library">
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
      {quotaPrompt && (
        <dialog
          ref={quotaDialogRef}
          className="map-dialog"
          aria-labelledby="quota-dialog-title"
          onCancel={(event) => {
            event.preventDefault();
            chooseQuota("cancel");
          }}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
            }}
          >
            <div className="map-dialog-head">
              <h3 id="quota-dialog-title">ScreenScraper limit</h3>
            </div>
            <p className="hint">
              {quotaPrompt.fits <= 0
                ? `ScreenScraper has no requests left today. ${quotaPrompt.games_left.toLocaleString()} games are still queued.`
                : `${quotaPrompt.games_left.toLocaleString()} games are still queued. ScreenScraper can take ${quotaPrompt.fits.toLocaleString()} more today. A cover counts as a second request.`}
            </p>
            <div className="quota-actions">
              <button type="button" onClick={() => chooseQuota("continue")}>
                Continue
              </button>
              {quotaPrompt.fits > 0 && (
                <button type="button" onClick={() => chooseQuota("limit")}>
                  Limit to {quotaPrompt.fits.toLocaleString()}
                </button>
              )}
              <button type="button" onClick={() => chooseQuota("cancel")}>
                Cancel
              </button>
            </div>
          </form>
        </dialog>
      )}
      {mapFile && (
        <dialog
          ref={mapDialogRef}
          className="map-dialog"
          aria-labelledby="map-dialog-title"
          onCancel={(event) => {
            event.preventDefault();
            closeMap();
          }}
          onClick={(event) => {
            if (event.target === event.currentTarget) closeMap();
          }}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void searchIgdb();
            }}
          >
            <div className="map-dialog-head">
              <h3 id="map-dialog-title">Choose on IGDB</h3>
              <button type="button" onClick={closeMap}>
                Close
              </button>
            </div>
            <p className="hint">Map {mapFile.name}. This replaces the saved match.</p>
            {mapNote && <p className={mapNote.error ? "error" : "hint"}>{mapNote.text}</p>}
            <div className="browse-bar">
              <input
                className="browse-search"
                aria-label="IGDB search"
                value={mapQuery}
                onChange={(event) => setMapQuery(event.currentTarget.value)}
                disabled={locked || mapBusy}
              />
              <button type="submit" disabled={locked || mapBusy}>
                Search IGDB
              </button>
            </div>
            <ul className="map-results">
              {mapHits.map((hit) => (
                <li key={hit.id}>
                  <span>
                    {hit.name}
                    {hit.year ? ` (${hit.year})` : ""} · {hit.category}
                  </span>
                  <button
                    type="button"
                    onClick={() => void assignIgdb(hit.id)}
                    disabled={locked || mapBusy}
                  >
                    Use this game
                  </button>
                </li>
              ))}
            </ul>
          </form>
        </dialog>
      )}

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
        <section className="panel browse-panel">
          <div className="browse-bar">
            <select
              aria-label="Platform"
              value={browsePlatform}
              onChange={(event) => {
                searchRequest.current += 1;
                setHits([]);
                setSelectedIds([]);
                setDetailId(null);
                setPage(0);
                setPageQuery(0);
                setBrowsePlatform(event.currentTarget.value);
              }}
              disabled={locked}
            >
              {(status?.platforms ?? [{ id: "snes", name: "Nintendo - Super Nintendo" }]).map((platform) => (
                <option key={platform.id} value={platform.id}>
                  {platform.name}
                </option>
              ))}
            </select>
            <input
              className="browse-search"
              aria-label="Search"
              value={query}
              onChange={(event) => {
                setQuery(event.currentTarget.value);
                setPage(0);
                setPageQuery(0);
              }}
              placeholder="Title, year, publisher, or file name"
            />
            <button
              type="button"
              title={browseView === "cards" ? "Switch to list" : "Switch to cards"}
              onClick={() => setBrowseView((current) => (current === "cards" ? "list" : "cards"))}
            >
              {browseView === "cards" ? "Cards" : "List"}
            </button>
            <Menu label="Scrape" align="end">
              <button
                type="button"
                role="menuitem"
                title="Look up every game that is still waiting."
                onClick={() => void enrich()}
                disabled={locked || !browsed || browsed.pending === 0}
              >
                Scrape all
                {browsed ? ` (${browsed.pending.toLocaleString()} waiting)` : ""}
              </button>
              <button
                type="button"
                role="menuitem"
                title="Look up the games in this list, including ones already scraped."
                onClick={() => void enrich(hits.map((hit) => hit.file_id))}
                disabled={locked || hits.length === 0}
              >
                Scrape shown ({hits.length.toLocaleString()})
              </button>
              <button
                type="button"
                role="menuitem"
                title="Look up the checked games, including ones already scraped."
                onClick={() => void enrich(selectedIds)}
                disabled={locked || selectedIds.length === 0}
              >
                Scrape selected ({selectedIds.length.toLocaleString()})
              </button>
            </Menu>
            <div className="browse-pager" role="navigation" aria-label="Pages">
              <button type="button" onClick={() => showPage(0)} disabled={locked || page <= 0}>
                First
              </button>
              <button
                type="button"
                aria-label="Previous page"
                title="Previous page"
                onClick={() => showPage(page - 1)}
                disabled={locked || page <= 0}
              >
                &lt;
              </button>
              <span className="browse-pager-range">
                {total === 0
                  ? "No games"
                  : `${(pageOffset + 1).toLocaleString()}–${(pageOffset + hits.length).toLocaleString()} of ${total.toLocaleString()}`}
              </span>
              <button
                type="button"
                aria-label="Next page"
                title="Next page"
                onClick={() => showPage(page + 1)}
                disabled={locked || page >= Math.max(0, Math.ceil(total / SEARCH_LIMIT) - 1)}
              >
                &gt;
              </button>
              <button
                type="button"
                onClick={() => showPage(Math.max(0, Math.ceil(total / SEARCH_LIMIT) - 1))}
                disabled={locked || page >= Math.max(0, Math.ceil(total / SEARCH_LIMIT) - 1)}
              >
                Last
              </button>
            </div>
          </div>
          {browseView === "cards" ? (
          <div className="browse-results" ref={browseRef}>
          <div className="game-grid">
            {hits.map((hit) => (
              <article
                key={hit.file_id}
                data-file-id={hit.file_id}
                className={hit.file_id === detailId ? "game-card active" : "game-card"}
              >
                {hit.thumb_path ? (
                  <img src={coverSrc(hit.thumb_path, thumbRev)} alt="" />
                ) : (
                  <div className="thumb-fallback">
                    {hit.needs_enrich ? "Not looked up" : "No cover"}
                  </div>
                )}
                <div className="game-card-body">
                  <div className="game-card-title">
                    <input
                      type="checkbox"
                      aria-label={`Select ${hit.title}`}
                      checked={selectedIds.includes(hit.file_id)}
                      onChange={() => toggleSelected(hit.file_id)}
                      disabled={locked}
                    />
                    <strong>{hit.title}</strong>
                  </div>
                  <p>
                    {[hit.year, hit.publisher, conditionLabel(hit)]
                      .filter(Boolean)
                      .join(" · ") || hit.name}
                  </p>
                  <div className="game-card-actions">
                    <button
                      type="button"
                      title="Look this game up again and replace the saved match."
                      onClick={() => void enrich([hit.file_id])}
                      disabled={locked}
                    >
                      Scrape
                    </button>
                    <button type="button" onClick={() => void openSearch(hit.title)} disabled={locked}>
                      Google
                    </button>
                    {browsePlatform === "pc" && (
                      <button
                        type="button"
                        onClick={() => openMap({ file_id: hit.file_id, name: hit.name, relative_path: hit.name })}
                        disabled={locked}
                      >
                        Choose on IGDB
                      </button>
                    )}
                    {pageLink(hit.link) && (
                      <button
                        type="button"
                        onClick={() => void openLink(hit.link ?? "")}
                        disabled={locked}
                      >
                        {pageLink(hit.link)?.label}
                      </button>
                    )}
                  </div>
                </div>
              </article>
            ))}
          </div>
          </div>
          ) : (
          <div className="browse-results game-browser" ref={browseRef}>
            <div
              className="game-list"
              onKeyDown={(event) => {
                if (event.altKey || event.ctrlKey || event.metaKey || hits.length === 0 || locked) return;
                const index = hits.findIndex((hit) => hit.file_id === detailId);
                const lastPage = Math.max(0, Math.ceil(total / SEARCH_LIMIT) - 1);
                if (event.key === "ArrowDown" && index === hits.length - 1 && page < lastPage) {
                  event.preventDefault();
                  showPage(page + 1, "start");
                  return;
                }
                if ((event.key === "ArrowUp" && (index === 0 || index < 0) && page > 0)) {
                  event.preventDefault();
                  showPage(page - 1, "end");
                  return;
                }
                let next = index;
                if (event.key === "ArrowDown") next = index < 0 ? 0 : Math.min(hits.length - 1, index + 1);
                else if (event.key === "ArrowUp") next = index < 0 ? 0 : Math.max(0, index - 1);
                else if (event.key === "Home") next = 0;
                else if (event.key === "End") next = hits.length - 1;
                else return;
                event.preventDefault();
                const hit = hits[next];
                if (!hit || hit.file_id === detailId) return;
                focusDetail.current = true;
                setDetailId(hit.file_id);
              }}
            >
              {hits.length === 0 ? (
                <p className="hint game-list-empty">No games to show.</p>
              ) : (
                hits.map((hit) => (
                  <div
                    key={hit.file_id}
                    data-file-id={hit.file_id}
                    className={
                      hit.file_id === detailId ? "game-list-row active" : "game-list-row"
                    }
                    onClick={(event) => {
                      setDetailId(hit.file_id);
                      if (event.target instanceof HTMLInputElement) return;
                      event.currentTarget.querySelector("button")?.focus();
                    }}
                  >
                    <input
                      type="checkbox"
                      aria-label={`Select ${hit.title}`}
                      checked={selectedIds.includes(hit.file_id)}
                      onClick={(event) => event.stopPropagation()}
                      onChange={() => toggleSelected(hit.file_id)}
                      disabled={locked}
                    />
                    <button
                      type="button"
                      className="game-list-copy"
                      tabIndex={hit.file_id === detailId ? 0 : -1}
                      aria-current={hit.file_id === detailId ? "true" : undefined}
                      onClick={() => setDetailId(hit.file_id)}
                    >
                      <strong>{hit.title}</strong>
                      <span>{hitSummary(hit)}</span>
                    </button>
                  </div>
                ))
              )}
            </div>
            <GameDetail
              hit={hits.find((hit) => hit.file_id === detailId) ?? null}
              platformName={browsed?.name ?? null}
              platformId={browsePlatform}
              thumbRev={thumbRev}
              locked={locked}
              onScrape={(fileId) => void enrich([fileId])}
              onSearch={(title) => void openSearch(title)}
              onOpenLink={(url) => void openLink(url)}
              onMap={(hit) =>
                openMap({ file_id: hit.file_id, name: hit.name, relative_path: hit.name })
              }
            />
          </div>
          )}
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
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={downloadCovers}
              disabled={locked}
              onChange={(event) => {
                const enabled = event.currentTarget.checked;
                setDownloadCovers(enabled);
                void invoke("library_set_download_covers", { enabled }).catch((saveError: unknown) => {
                  showError("settings", saveError);
                  void refresh();
                });
              }}
            />
            Download covers
          </label>
          <p className="hint">
            Turn this off for a text-only scrape. Each cover is a separate ScreenScraper request.
            Scraping a game again with this off removes its saved cover.
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
          <h2 className="panel-title">IGDB</h2>
          <p className="hint">
            {status?.igdb_credentials_saved
              ? "Saved in the app data folder. PC scrapes ask IGDB first, then ScreenScraper."
              : "Client ID and Client Secret come from the Twitch application. PC scrapes ask IGDB first, then ScreenScraper."}
          </p>
          <div className="two-col">
            <label>
              Client ID
              <input
                value={igdbCreds.client_id}
                onChange={(event) =>
                  setIgdbCreds({ ...igdbCreds, client_id: event.currentTarget.value })
                }
                disabled={locked}
                autoComplete="off"
              />
            </label>
            <label>
              Client Secret
              <input
                type="password"
                value={igdbCreds.client_secret}
                onChange={(event) =>
                  setIgdbCreds({ ...igdbCreds, client_secret: event.currentTarget.value })
                }
                disabled={locked}
                autoComplete="off"
              />
            </label>
          </div>
          <div className="actions">
            <button type="button" onClick={() => void saveIgdbCredentials()} disabled={locked}>
              Save IGDB credentials
            </button>
          </div>
          <h2 className="panel-title">Index</h2>
          <p className="hint">
            {status
              ? `${status.db_path}${status.exists ? "" : " (not created yet)"}`
              : "Loading the library…"}
          </p>
          <div className="actions">
            <button type="button" onClick={() => void openDbFolder()} disabled={locked || !status}>
              Open folder
            </button>
            <button type="button" onClick={() => void deleteIndex()} disabled={locked || !status?.exists}>
              Delete index
            </button>
          </div>
        </section>
      )}

      {screen === "unmapped" && (
        <section className="panel">
          <h2 className="panel-title">Unmapped</h2>
          <p className="hint">
            These files were scraped and nothing safe matched. A PC game is kept only when the file
            name and the catalog title are the same game. IGDB is tried first, then ScreenScraper.
            Arcade still comes from the MAME list; a cover miss does not land here.
          </p>
          <div className="browse-bar">
            <select
              aria-label="Platform"
              value={unmappedPlatform}
              onChange={(event) => {
                setUnmappedPlatform(event.currentTarget.value);
                setUnmappedSelected([]);
              }}
              disabled={locked}
            >
              {(status?.platforms ?? []).map((platform) => (
                <option key={platform.id} value={platform.id}>
                  {platform.name}
                </option>
              ))}
            </select>
            <label className="unmapped-select-all">
              <input
                ref={unmappedSelectAllRef}
                type="checkbox"
                checked={allUnmappedSelected}
                onChange={() => setUnmappedSelected(allUnmappedSelected ? [] : unmappedShownIds)}
                disabled={locked || unmappedShownIds.length === 0}
              />
              Select all
            </label>
            <button
              type="button"
              className="unmapped-scrape"
              disabled={locked || unmappedSelected.length === 0}
              onClick={() =>
                void enrich(unmappedSelected, {
                  screen: "unmapped",
                  platform: unmappedPlatform,
                })
              }
            >
              Scrape selected ({unmappedSelected.length.toLocaleString()})
            </button>
          </div>
          <p className="hint">
            {unmapped
              ? `${unmapped.total.toLocaleString()} unmapped${
                  unmapped.files.length < unmapped.total
                    ? `, showing the first ${unmapped.files.length.toLocaleString()}`
                    : ""
                }`
              : "Loading…"}
          </p>
          <ul className="unmapped-list">
            {(unmapped?.files ?? []).map((file) => (
              <li key={file.file_id} className={mapFile?.file_id === file.file_id ? "mapping" : undefined}>
                <label>
                  <input
                    type="checkbox"
                    checked={unmappedSelected.includes(file.file_id)}
                    onChange={() =>
                      setUnmappedSelected((current) =>
                        current.includes(file.file_id)
                          ? current.filter((id) => id !== file.file_id)
                          : [...current, file.file_id],
                      )
                    }
                    disabled={locked}
                  />
                </label>
                <button
                  type="button"
                  className="unmapped-name"
                  onClick={() => openMap(file)}
                  disabled={locked}
                >
                  {file.name}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      {screen === "limits" && (
        <section className="panel">
          <h2 className="panel-title">ScreenScraper limits</h2>
          <p className="hint">
            These stay unknown until ScreenScraper sends them. A scrape updates the numbers from
            the member account.
          </p>
          <dl className="limit-list">
            <dt>Requests per minute</dt>
            <dd>{limitValue(status?.screenscraper.max_per_minute ?? null)}</dd>
            <dt>Requests per day</dt>
            <dd>{limitValue(status?.screenscraper.max_per_day ?? null)}</dd>
            <dt>Used today</dt>
            <dd>{limitValue(status?.screenscraper.used_today ?? null)}</dd>
            <dt>Unrecognized per day</dt>
            <dd>{limitValue(status?.screenscraper.max_misses_per_day ?? null)}</dd>
            <dt>Unrecognized today</dt>
            <dd>{limitValue(status?.screenscraper.misses_today ?? null)}</dd>
            <dt>Threads</dt>
            <dd>{limitValue(status?.screenscraper.max_threads ?? null)}</dd>
            <dt>Download speed</dt>
            <dd>{limitValue(status?.screenscraper.max_download_kbps ?? null, " KB/s")}</dd>
          </dl>
        </section>
      )}

      {screen === "logs" && (
        <section className="panel">
          <h2 className="panel-title">Logs</h2>
          <p className="hint">
            The latest {LOG_LIMIT} lines from this session. A scrape writes which source matched
            and whether it replaced a saved game.
          </p>
          <div className="actions">
            <button type="button" onClick={() => setLogs([])} disabled={logs.length === 0}>
              Clear logs
            </button>
          </div>
          {logs.length === 0 ? (
            <p className="hint">No log lines yet.</p>
          ) : (
            <ul className="log-list" ref={logListRef}>
              {logs.map((line, index) => (
                <li key={`${index}-${line}`}>{line}</li>
              ))}
            </ul>
          )}
        </section>
      )}
    </div>
  );
}

function coverSrc(path: string, rev: number): string {
  const src = convertFileSrc(path);
  return `${src}${src.includes("?") ? "&" : "?"}v=${rev}`;
}

function pageLink(url: string | null): { label: string } | null {
  if (!url) return null;
  const igdb = "https://www.igdb.com/games/";
  if (url.startsWith(igdb)) {
    const slug = url.slice(igdb.length);
    if (slug.length > 0 && !slug.includes("/")) return { label: "IGDB" };
  }
  if (
    url.startsWith("https://adb.arcadeitalia.net/") ||
    url.startsWith("http://adb.arcadeitalia.net/")
  ) {
    return { label: "Arcade Database" };
  }
  return null;
}

function sourceLabel(provider: string | null, id: string | null): string | null {
  if (!provider) return null;
  const name =
    provider === "igdb"
      ? "IGDB"
      : provider === "screenscraper"
        ? "ScreenScraper"
        : provider === "mame"
          ? "MAME list"
          : provider;
  return id ? `${name} ${id}` : name;
}

function limitValue(value: number | null, suffix = ""): string {
  if (value == null) return "Unknown";
  return `${value.toLocaleString()}${suffix}`;
}

function versionDraftsFrom(status: LibraryStatus): Record<string, string> {
  return Object.fromEntries(
    status.platforms.map((platform) => [platform.id, platform.mame_version ?? ""]),
  );
}

function hitSummary(hit: BrowseHit): string {
  return [hit.year, hit.publisher, conditionLabel(hit)].filter(Boolean).join(" · ") || hit.name;
}

function detailFields(hit: BrowseHit, platformName: string | null): Array<[string, string]> {
  const fields: Array<[string, string | null]> = [
    ["Platform", platformName],
    ["File", hit.name],
    ["Year", hit.year],
    ["Publisher", hit.publisher],
    ["Status", conditionLabel(hit)],
    ["Source", sourceLabel(hit.provider, hit.provider_id)],
    ["Lookup", hit.needs_enrich ? "Waiting to scrape" : "Scraped"],
  ];
  return fields.filter((field): field is [string, string] => Boolean(field[1]));
}

function GameDetail({
  hit,
  platformName,
  platformId,
  thumbRev,
  locked,
  onScrape,
  onSearch,
  onOpenLink,
  onMap,
}: {
  hit: BrowseHit | null;
  platformName: string | null;
  platformId: string;
  thumbRev: number;
  locked: boolean;
  onScrape: (fileId: number) => void;
  onSearch: (title: string) => void;
  onOpenLink: (url: string) => void;
  onMap: (hit: BrowseHit) => void;
}) {
  if (!hit) {
    return (
      <div className="game-detail">
        <p className="hint">Select a game to see its details.</p>
      </div>
    );
  }
  return (
    <div className="game-detail">
      <h3>{hit.title}</h3>
      <div className="game-detail-art">
        {hit.thumb_path ? (
          <img src={coverSrc(hit.thumb_path, thumbRev)} alt="" />
        ) : (
          <div className="thumb-fallback game-detail-cover">
            {hit.needs_enrich ? "Not looked up" : "No cover"}
          </div>
        )}
      </div>
      <dl>
        {detailFields(hit, platformName).map(([label, value]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
      <div className="game-card-actions">
        <button
          type="button"
          title="Look this game up again and replace the saved match."
          onClick={() => onScrape(hit.file_id)}
          disabled={locked}
        >
          Scrape
        </button>
        <button type="button" onClick={() => onSearch(hit.title)} disabled={locked}>
          Google
        </button>
        {platformId === "pc" && (
          <button type="button" onClick={() => onMap(hit)} disabled={locked}>
            Choose on IGDB
          </button>
        )}
        {pageLink(hit.link) && (
          <button type="button" onClick={() => onOpenLink(hit.link ?? "")} disabled={locked}>
            {pageLink(hit.link)?.label}
          </button>
        )}
      </div>
    </div>
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
