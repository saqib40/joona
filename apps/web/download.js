/*
 * Platform-aware download links.
 *
 * Finds the newest published GitHub release and points the download button at
 * the installer for the visitor's OS: the NSIS `.exe` on Windows and the `.dmg`
 * on macOS. The result is cached for an hour so repeat visits don't spend the
 * unauthenticated GitHub API rate limit. If the API fails, or a release has no
 * asset for the visitor's platform, the link falls back to the release page.
 */
(function () {
  "use strict";

  var REPO = "saqib40/joona";
  var RELEASES_PAGE = "https://github.com/" + REPO + "/releases/latest";
  var LATEST_API = "https://api.github.com/repos/" + REPO + "/releases/latest";
  var CACHE_KEY = "joona.latest-release";
  var CACHE_TTL = 5 * 60 * 1000; // 5 minutes cache

  var os = detectOS();
  var arch = "";

  // The commands page highlights the key column for the visitor's platform.
  if (os === "windows" || os === "macos") {
    document.documentElement.dataset.platform = os;
  }

  function detectOS() {
    var platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    var ua = navigator.userAgent || "";

    // iPadOS 13+ asks for the desktop site and reports MacIntel, but it can't
    // install a Mac app.
    if (/mac/i.test(platform) && navigator.maxTouchPoints > 1) return "other";
    if (/mac/i.test(platform) || /Mac OS X|Macintosh/i.test(ua)) return "macos";
    if (/win/i.test(platform) || /Windows/i.test(ua)) return "windows";
    return "other";
  }

  function detectArch() {
    if (navigator.userAgentData && navigator.userAgentData.getHighEntropyValues) {
      return navigator.userAgentData
        .getHighEntropyValues(["architecture"])
        .then(function (values) {
          return values.architecture || "";
        })
        .catch(function () {
          return "";
        });
    }
    return Promise.resolve(/arm64|aarch64/i.test(navigator.userAgent) ? "arm64" : "");
  }

  function isReleaseAsset(url) {
    return (
      typeof url === "string" &&
      url.indexOf("https://github.com/" + REPO + "/releases/download/") === 0
    );
  }

  function scoreWindows(name) {
    if (!/\.(exe|msi)$/i.test(name)) return -1;
    if (/\.exe$/i.test(name) && /setup/i.test(name)) return 3;
    if (/\.exe$/i.test(name)) return 2;
    return 1;
  }

  function scoreMac(name) {
    if (!/\.dmg$/i.test(name)) return -1;
    var arm = /aarch64|arm64/i.test(name);
    var x64 = /x64|x86_64|amd64|intel/i.test(name);
    var universal = /universal/i.test(name);
    if (universal) return 6;
    if (arch === "x86" || arch === "x86_64" || arch === "x64") return x64 ? 5 : arm ? 2 : 3;
    // Apple Silicon is the common case and the only macOS runner we build on.
    return arm ? 5 : x64 ? 3 : 2;
  }

  function pickAsset(assets, platform) {
    var best = null;
    var bestScore = 0;
    for (var i = 0; i < assets.length; i += 1) {
      var asset = assets[i];
      var score = platform === "windows" ? scoreWindows(asset.name) : scoreMac(asset.name);
      if (score > bestScore) {
        best = asset;
        bestScore = score;
      }
    }
    return best;
  }

  function normalizeRelease(json) {
    var assets = [];
    var raw = (json && json.assets) || [];
    for (var i = 0; i < raw.length; i += 1) {
      var asset = raw[i];
      if (asset && asset.name && isReleaseAsset(asset.browser_download_url)) {
        assets.push({ name: asset.name, url: asset.browser_download_url });
      }
    }
    return {
      version: String((json && json.tag_name) || "").replace(/^v/, ""),
      page: (json && json.html_url) || RELEASES_PAGE,
      assets: assets,
      savedAt: Date.now(),
    };
  }

  function readCache() {
    try {
      var raw = localStorage.getItem(CACHE_KEY);
      if (!raw) return null;
      var cached = JSON.parse(raw);
      if (!cached || !Array.isArray(cached.assets) || Date.now() - cached.savedAt > CACHE_TTL) {
        return null;
      }
      return cached;
    } catch (_) {
      return null;
    }
  }

  function writeCache(release) {
    try {
      localStorage.setItem(CACHE_KEY, JSON.stringify(release));
    } catch (_) {
      // Storage can be unavailable in private modes; the page still works.
    }
  }

  function fetchFreshRelease() {
    return fetch(LATEST_API, { headers: { Accept: "application/vnd.github+json" } })
      .then(function (response) {
        if (!response.ok) throw new Error("GitHub returned " + response.status);
        return response.json();
      })
      .then(function (json) {
        var release = normalizeRelease(json);
        writeCache(release);
        return release;
      })
      .catch(function () {
        return null;
      });
  }

  function loadRelease(onUpdate) {
    var cached = readCache();
    if (cached) {
      // Revalidate in background if cache is older than 1 minute
      if (Date.now() - cached.savedAt > 60 * 1000) {
        fetchFreshRelease().then(function (fresh) {
          if (fresh && fresh.version !== cached.version && typeof onUpdate === "function") {
            onUpdate(fresh);
          }
        });
      }
      return Promise.resolve(cached);
    }

    return fetchFreshRelease();
  }

  function setLinks(href) {
    var links = document.querySelectorAll("[data-download-link]");
    for (var i = 0; i < links.length; i += 1) {
      links[i].href = href;
    }
  }

  function apply(release) {
    var asset = release && (os === "windows" || os === "macos") ? pickAsset(release.assets, os) : null;
    var label = document.querySelector("[data-download-label]");
    var meta = document.querySelector("[data-download-meta]");
    var href = (asset && asset.url) || (release && release.page) || RELEASES_PAGE;

    setLinks(href);

    if (!label) return;

    if (os === "macos" || os === "windows") {
      var name = os === "macos" ? "macOS" : "Windows";
      if (asset) {
        label.textContent = "Download for " + name;
      } else if (release) {
        label.textContent = "See all downloads";
      } else {
        label.textContent = "Download for " + name;
      }
    }

    if (!meta) return;

    if (!release) {
      meta.textContent = "Windows and macOS.";
    } else if (asset) {
      meta.textContent = "Latest release v" + release.version + " for " + (os === "macos" ? "macOS" : "Windows") + ".";
    } else if (os === "macos" || os === "windows") {
      meta.textContent = "Latest release v" + release.version + ". This release doesn't include a " + (os === "macos" ? "macOS" : "Windows") + " installer yet.";
    } else {
      meta.textContent = "Latest release v" + release.version + ". Joona is available for Windows and macOS.";
    }
  }

  detectArch()
    .then(function (detected) {
      arch = detected;
      return loadRelease(apply);
    })
    .then(apply)
    .catch(function () {
      // Keep the static fallback links already in the markup.
    });
})();
