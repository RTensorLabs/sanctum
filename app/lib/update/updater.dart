import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

import '../background/background.dart';
import '../identity/secrets.dart';
import '../net/net_bus.dart';
import '../src/rust/api/session.dart';
import '../src/rust/api/update.dart';

/// Over-the-air updates from the project's GitHub releases, checked and
/// downloaded through Tor. Android installs an update only if it's signed
/// with the same key as this app.
class Updater {
  Updater._() : this.forTesting();

  @visibleForTesting
  Updater.forTesting({
    Future<Directory> Function() cacheDirectory = getApplicationCacheDirectory,
    Future<void> Function({required String url, required String path}) download = updateDownload,
    Future<double> Function() readProgress = updateProgress,
    Future<String> Function(String path) openInstaller = Background.installApk,
    Future<int> Function() buildNumber = Background.buildNumber,
    Future<UpdateInfo?> Function(int current)? findUpdate,
    Future<bool> Function()? automaticChecks,
    bool Function()? online,
    DateTime Function() now = DateTime.now,
  })  : _cacheDirectory = cacheDirectory,
        _download = download,
        _readProgress = readProgress,
        _openInstaller = openInstaller,
        _buildNumber = buildNumber,
        _findUpdate = findUpdate ?? ((current) => updateCheck(current: current)),
        _automaticChecks = automaticChecks ?? (() => loadFlag('auto_update_check', fallback: true)),
        _online = online ?? (() => NetBus.instance.status.value.status == NetStatus.online),
        _now = now;

  final Future<Directory> Function() _cacheDirectory;
  final Future<void> Function({required String url, required String path}) _download;
  final Future<double> Function() _readProgress;
  final Future<String> Function(String path) _openInstaller;
  final Future<int> Function() _buildNumber;
  final Future<UpdateInfo?> Function(int current) _findUpdate;
  final Future<bool> Function() _automaticChecks;
  final bool Function() _online;
  final DateTime Function() _now;

  static final instance = Updater._();

  static const _checkEvery = Duration(minutes: 5);
  static const _resumeAfter = Duration(minutes: 1);

  /// After a failed check (Tor can be slow or flaky), try again this soon.
  static const _retryAfter = Duration(minutes: 1);

  /// A newer release, once found.
  final available = ValueNotifier<UpdateInfo?>(null);

  /// 0–1 while downloading; null otherwise.
  final progress = ValueNotifier<double?>(null);

  /// The last problem, to show; cleared by the next attempt.
  final error = ValueNotifier<String?>(null);
  final checking = ValueNotifier<bool>(false);
  final installing = ValueNotifier<bool>(false);
  final lastChecked = ValueNotifier<DateTime?>(null);

  int current = 0;
  DateTime? _lastFailure;
  Future<void>? _checkInFlight;
  bool _forced = false;
  Timer? _timer;

  /// Keeps checking while the app runs: the app can stay open for days
  /// (it keeps you reachable in the background), and a single check when
  /// Tor first connects may fail.
  void start() {
    _timer ??= Timer.periodic(const Duration(minutes: 1), (_) => check());
  }

  /// Checks every five minutes, or when returning after a minute away.
  /// An explicit check bypasses the cooldown. Concurrent callers wait for
  /// the same result rather than reporting "up to date" before it finishes.
  Future<void> check({bool force = false, bool onResume = false}) {
    if (_checkInFlight != null) {
      if (force) _forced = true;
      return _checkInFlight!;
    }
    if (installing.value) return Future.value();
    // Not connected yet: checking now could only fail. Connecting checks.
    if (!_online()) {
      if (force) error.value = 'Sanctum is not connected yet. Try again once it is connected.';
      return Future.value();
    }
    final now = _now();
    if (!force) {
      if (_lastFailure != null) {
        if (now.difference(_lastFailure!) < _retryAfter) return Future.value();
      } else if (lastChecked.value != null &&
          now.difference(lastChecked.value!) < (onResume ? _resumeAfter : _checkEvery)) {
        return Future.value();
      }
    }
    _forced = force;
    checking.value = true;
    error.value = null;
    return _checkInFlight = _performCheck().whenComplete(() {
      _checkInFlight = null;
      _forced = false;
      checking.value = false;
    });
  }

  Future<void> _performCheck() async {
    try {
      current = await _buildNumber();
      if (!await _automaticChecks() && !_forced) return;
      available.value = await _checkWithRetries();
      lastChecked.value = _now();
      _lastFailure = null;
      error.value = null;
    } catch (_) {
      _lastFailure = _now();
      error.value = 'Couldn\'t check for updates. Check your connection and try again.';
    }
  }

  /// A Tor connection sometimes fails on its own; the next one usually works.
  Future<UpdateInfo?> _checkWithRetries() async {
    for (var attempt = 1;; attempt++) {
      try {
        return await _findUpdate(current);
      } catch (_) {
        if (attempt == 3) rethrow;
        await Future<void>.delayed(Duration(seconds: 10 * attempt));
      }
    }
  }

  /// Downloads (if not yet) and opens Android's installer.
  Future<void> install() async {
    final update = available.value;
    if (update == null || installing.value || checking.value) return;
    // Take the guard before the first await, including cache lookups and
    // reopening a download after Android asks for install permission.
    installing.value = true;
    error.value = null;
    var problem = 'Couldn\'t prepare the update. Check your phone\'s free space and try again.';
    try {
      final dir = Directory('${(await _cacheDirectory()).path}/updates');
      final path = '${dir.path}/sanctum-${update.build}.apk';
      if (!File(path).existsSync()) {
        // Older downloads are no use any more.
        if (dir.existsSync()) dir.deleteSync(recursive: true);
        progress.value = 0;
        problem = 'The download didn\'t finish. Check your connection and tap Update again.';
        var active = true;
        var reading = false;
        final poll = Timer.periodic(const Duration(milliseconds: 400), (_) async {
          if (reading) return;
          reading = true;
          try {
            final value = await _readProgress();
            // Cancelling a timer doesn't cancel an in-flight native call.
            // A late result must not put the app back into "downloading".
            if (active) progress.value = value.clamp(0, 1);
          } catch (_) {
            // A missed progress sample does not mean the download failed.
          } finally {
            reading = false;
          }
        });
        try {
          await _download(url: update.url, path: path);
        } finally {
          active = false;
          poll.cancel();
        }
      }
      progress.value = null;
      problem = 'Couldn\'t open the installer. Tap Update to try again.';
      final result = await _openInstaller(path);
      if (result == 'permission') {
        error.value = 'Allow Sanctum to install apps, then tap Update again.';
      } else if (result != 'started') {
        error.value = 'Couldn\'t open the installer.';
      }
    } catch (_) {
      error.value = problem;
    } finally {
      progress.value = null;
      installing.value = false;
    }
  }
}
