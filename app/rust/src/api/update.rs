//! Updates over the air: the newest release on GitHub, checked and
//! downloaded through Tor (GitHub sees a Tor exit, never this phone).
//! Android installs it only if it's signed with the same key as the app,
//! so a tampered download can't be installed.

use super::session::runtime;
use sanctum_net::https::{self, Connect};
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::task::Poll;
use std::time::{Duration, Instant};

/// Where releases are published, in the order they're tried. The project
/// moved to RTensorLabs; the old address (which GitHub redirects for now) is
/// only a fallback for when the new one can't be reached. Whichever answers,
/// Android installs the download only if it's signed with this app's key.
const REPOS: [&str; 2] = ["https://github.com/RTensorLabs/sanctum", "https://github.com/Tejascodex001/sanctum"];
/// The APK each release carries (see .github/workflows/android.yml).
const ASSET: &str = "app-release.apk";
const MAX_APK: usize = 200 << 20;
/// A check that takes longer has hung (a stuck Tor circuit): give up, so
/// the next check can run.
const CHECK_TIMEOUT: Duration = Duration::from_secs(60);
/// A download that receives nothing for this long has hung.
const DOWNLOAD_STALL: Duration = Duration::from_secs(120);
/// Even a slow download that keeps going gives up after this long, so a
/// trickle can't hold the update forever; the next attempt starts afresh.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Set once Tor is up.
pub(crate) static CONNECT: LazyLock<Mutex<Option<Connect>>> = LazyLock::new(Default::default);
static RECEIVED: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicU64 = AtomicU64::new(0);

pub struct UpdateInfo {
    pub build: i64,
    pub url: String,
    /// What changed since `current`, newest first.
    pub notes: Vec<ReleaseNotes>,
}

pub struct ReleaseNotes {
    pub build: i64,
    /// Markdown: "- **Change**" lines with "  - detail" under them.
    pub text: String,
}

/// The "## Build N" sections of a changelog newer than `current`.
fn notes_since(changelog: &str, current: i64) -> Vec<ReleaseNotes> {
    changelog
        .split("\n## Build ")
        .skip(1)
        .filter_map(|section| {
            let (number, text) = section.split_once('\n').unwrap_or((section, ""));
            let build = number.trim().parse::<i64>().ok()?;
            (build > current).then(|| ReleaseNotes {
                build,
                text: text.trim().to_string(),
            })
        })
        .collect()
}

fn connect() -> anyhow::Result<Connect> {
    CONNECT.lock().unwrap().clone().ok_or_else(|| anyhow::anyhow!("Tor isn't connected yet"))
}

/// Tries `attempt` with each address in `repos`, in order, and returns the
/// first success. If none works, the error is the first address's.
async fn first_reachable<T, F, Fut>(repos: &[&'static str], mut attempt: F) -> anyhow::Result<T>
where
    F: FnMut(&'static str) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let mut first_error = None;
    for &repo in repos {
        match attempt(repo).await {
            Ok(found) => return Ok(found),
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    Err(first_error.unwrap_or_else(|| anyhow::anyhow!("no release address")))
}

/// The build number of a `releases/tag/build-N` page.
fn build_of(release_page: &str) -> anyhow::Result<(String, i64)> {
    let tag = release_page.rsplit('/').next().unwrap_or_default();
    match tag.strip_prefix("build-").and_then(|n| n.parse::<i64>().ok()) {
        Some(build) => Ok((tag.to_string(), build)),
        None => anyhow::bail!("unexpected release: {release_page}"),
    }
}

/// Whether `url` is a release download from one of [`REPOS`].
fn is_release_download(url: &str) -> bool {
    REPOS.iter().any(|repo| url.strip_prefix(repo).is_some_and(|rest| rest.starts_with("/releases/download/")))
}

/// The newest release, if it's newer than `current` (this app's build).
/// Uses `releases/latest`, which redirects to the newest release's page:
/// no API, so no rate limits on shared Tor exits.
pub fn update_check(current: i64) -> anyhow::Result<Option<UpdateInfo>> {
    let connect = connect()?;
    let (repo, tag, build) = runtime().block_on(first_reachable(&REPOS, |repo| {
        let connect = connect.clone();
        async move {
            let latest =
                tokio::time::timeout(CHECK_TIMEOUT, https::redirect_of(&connect, &format!("{repo}/releases/latest"))).await??;
            let (tag, build) = build_of(&latest)?;
            anyhow::Ok((repo, tag, build))
        }
    }))?;
    if build <= current {
        return Ok(None);
    }
    // Older releases have no changelog; the update is still offered.
    let url = format!("{repo}/releases/download/{tag}/changelog.md");
    let changelog = runtime()
        .block_on(async { tokio::time::timeout(CHECK_TIMEOUT, https::get(&connect, &url, 1 << 20, &|_, _| {})).await })
        .ok()
        .and_then(Result::ok)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    Ok(Some(UpdateInfo {
        build,
        url: format!("{repo}/releases/download/{tag}/{ASSET}"),
        notes: notes_since(&changelog, current),
    }))
}

/// When a download last received something.
struct Activity {
    start: Instant,
    /// Milliseconds after `start`.
    last: AtomicU64,
}

impl Activity {
    fn new() -> Self {
        Self { start: Instant::now(), last: AtomicU64::new(0) }
    }

    fn touch(&self) {
        self.last.store(self.start.elapsed().as_millis() as u64, Ordering::Relaxed);
    }

    fn idle(&self) -> Duration {
        self.start.elapsed().saturating_sub(Duration::from_millis(self.last.load(Ordering::Relaxed)))
    }
}

/// Runs `work`, giving up if `activity` shows nothing for `stall`, or after
/// `overall` in any case.
async fn bounded<T>(
    work: impl Future<Output = anyhow::Result<T>>,
    activity: &Activity,
    stall: Duration,
    overall: Duration,
) -> anyhow::Result<T> {
    let watchdog = async {
        loop {
            tokio::time::sleep(stall / 4).await;
            if activity.idle() >= stall {
                return;
            }
        }
    };
    let mut work = std::pin::pin!(work);
    let mut watchdog = std::pin::pin!(watchdog);
    let raced = std::future::poll_fn(|cx| {
        if let Poll::Ready(done) = work.as_mut().poll(cx) {
            return Poll::Ready(Some(done));
        }
        if watchdog.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    });
    match tokio::time::timeout(overall, raced).await {
        Ok(Some(done)) => done,
        Ok(None) => anyhow::bail!("the download stopped"),
        Err(_) => anyhow::bail!("the download took too long"),
    }
}

/// Downloads the update to `path` (progress: [`update_progress`]).
pub fn update_download(url: String, path: String) -> anyhow::Result<()> {
    anyhow::ensure!(is_release_download(&url), "not a Sanctum release");
    let connect = connect()?;
    RECEIVED.store(0, Ordering::Relaxed);
    TOTAL.store(0, Ordering::Relaxed);
    let activity = Activity::new();
    let progress = |got, total: Option<u64>| {
        activity.touch();
        RECEIVED.store(got, Ordering::Relaxed);
        TOTAL.store(total.unwrap_or(0), Ordering::Relaxed);
    };
    let download = https::get(&connect, &url, MAX_APK, &progress);
    let apk = runtime().block_on(bounded(download, &activity, DOWNLOAD_STALL, DOWNLOAD_TIMEOUT))?;
    anyhow::ensure!(apk.starts_with(b"PK"), "not an APK");
    if let Some(dir) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = format!("{path}.part");
    std::fs::write(&partial, apk)?;
    std::fs::rename(partial, path)?;
    Ok(())
}

/// 0–1 while downloading (0 until the size is known).
pub fn update_progress() -> f64 {
    let total = TOTAL.load(Ordering::Relaxed);
    if total == 0 {
        return 0.0;
    }
    RECEIVED.load(Ordering::Relaxed) as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_newer_than_installed() {
        let log = "# Sanctum changelog\n\n## Build 27\n\n- **Updates**\n  - over Tor\n\n## Build 26\n\n- **Nearby**\n\n## Build 25\n\n- **Groups**\n";
        let notes = notes_since(log, 25);
        assert_eq!(notes.iter().map(|n| n.build).collect::<Vec<_>>(), [27, 26]);
        assert_eq!(notes[0].text, "- **Updates**\n  - over Tor");
        assert!(notes_since(log, 27).is_empty());
        assert!(notes_since("", 1).is_empty());
    }

    fn block_on<F: Future>(work: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap().block_on(work)
    }

    #[test]
    fn new_address_is_tried_first() {
        assert_eq!(REPOS[0], "https://github.com/RTensorLabs/sanctum");
        let tried = Mutex::new(Vec::new());
        let found = block_on(first_reachable(&REPOS, |repo| {
            tried.lock().unwrap().push(repo);
            async move { anyhow::Ok(repo) }
        }));
        assert_eq!(found.unwrap(), REPOS[0]);
        // The old address isn't asked at all when the new one answers.
        assert_eq!(*tried.lock().unwrap(), [REPOS[0]]);
    }

    #[test]
    fn old_address_is_a_fallback() {
        let tried = Mutex::new(Vec::new());
        let found = block_on(first_reachable(&REPOS, |repo| {
            tried.lock().unwrap().push(repo);
            async move {
                if repo == REPOS[0] {
                    anyhow::bail!("HTTP 404 from github.com: Not Found");
                }
                anyhow::Ok(repo)
            }
        }));
        assert_eq!(found.unwrap(), REPOS[1]);
        assert_eq!(*tried.lock().unwrap(), REPOS);
    }

    #[test]
    fn nothing_reachable_reports_the_new_address_error() {
        let found: anyhow::Result<()> = block_on(first_reachable(&REPOS, |repo| async move {
            anyhow::bail!("failed: {repo}")
        }));
        assert_eq!(found.unwrap_err().to_string(), format!("failed: {}", REPOS[0]));
    }

    #[test]
    fn release_pages() {
        let (tag, build) = build_of("https://github.com/RTensorLabs/sanctum/releases/tag/build-83").unwrap();
        assert_eq!((tag.as_str(), build), ("build-83", 83));
        // Without releases, `latest` lands on the releases list.
        assert!(build_of("https://github.com/RTensorLabs/sanctum/releases").is_err());
        assert!(build_of("https://github.com/RTensorLabs/sanctum/releases/tag/v1.0").is_err());
        assert!(build_of("").is_err());
    }

    #[test]
    fn downloads_only_from_our_releases() {
        assert!(is_release_download("https://github.com/RTensorLabs/sanctum/releases/download/build-83/app-release.apk"));
        assert!(is_release_download("https://github.com/Tejascodex001/sanctum/releases/download/build-83/app-release.apk"));
        assert!(!is_release_download("https://github.com/RTensorLabs/sanctum-evil/releases/download/build-83/app-release.apk"));
        assert!(!is_release_download("https://github.com/RTensorLabs/sanctum.evil.example/releases/download/x"));
        assert!(!is_release_download("https://github.com/someone/sanctum/releases/download/build-83/app-release.apk"));
        assert!(!is_release_download("https://github.com/RTensorLabs/sanctum/archive/main.zip"));
        assert!(!is_release_download("http://github.com/RTensorLabs/sanctum/releases/download/build-83/app-release.apk"));
    }

    #[test]
    fn a_stalled_download_gives_up() {
        let activity = Activity::new();
        let started = Instant::now();
        let result: anyhow::Result<()> = block_on(bounded(
            std::future::pending(),
            &activity,
            Duration::from_millis(80),
            Duration::from_secs(30),
        ));
        assert_eq!(result.unwrap_err().to_string(), "the download stopped");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_download_that_keeps_going_finishes() {
        let activity = Activity::new();
        // Slower in total than the stall limit, but never quiet for that long.
        let work = async {
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_millis(30)).await;
                activity.touch();
            }
            anyhow::Ok(7)
        };
        let result = block_on(bounded(work, &activity, Duration::from_millis(150), Duration::from_secs(30)));
        assert_eq!(result.unwrap(), 7);
    }

    #[test]
    fn a_trickling_download_hits_the_overall_limit() {
        let activity = Activity::new();
        let work = async {
            loop {
                tokio::time::sleep(Duration::from_millis(10)).await;
                activity.touch();
            }
        };
        let result: anyhow::Result<()> =
            block_on(bounded(work, &activity, Duration::from_secs(10), Duration::from_millis(200)));
        assert_eq!(result.unwrap_err().to_string(), "the download took too long");
    }
}
