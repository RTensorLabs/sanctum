import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sanctum/settings/connection_settings.dart';
import 'package:sanctum/src/rust/api/update.dart';
import 'package:sanctum/theme/sigil_theme.dart';
import 'package:sanctum/update/updater.dart';
import 'home_test.dart' show loadFonts;

final _shotKey = GlobalKey();

Future<void> _shot(WidgetTester tester, String name) async {
  final dir = Platform.environment['SCREENSHOT_DIR'];
  if (dir == null) return;
  await tester.runAsync(() async {
    final boundary = _shotKey.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await boundary.toImage(pixelRatio: 2);
    final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
    File('$dir/$name.png').writeAsBytesSync(bytes!.buffer.asUint8List());
  });
}

const release = UpdateInfo(
    build: 80, url: 'https://github.com/Tejascodex001/sanctum/releases/download/build-80/app-release.apk', notes: []);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(loadFonts);

  test('a newly published release is detected after five minutes, without manual checks', () async {
    var now = DateTime(2026, 11, 1);
    var requests = 0;
    final u = Updater.forTesting(
      now: () => now,
      online: () => true,
      buildNumber: () async => 79,
      automaticChecks: () async => true,
      findUpdate: (_) async => ++requests == 1 ? null : release,
    );
    await u.check();
    expect(u.available.value, isNull);
    now = now.add(const Duration(minutes: 1));
    await u.check();
    expect(requests, 1);
    now = now.add(const Duration(minutes: 4));
    await u.check();
    expect(requests, 2);
    expect(u.available.value, release);
  });

  test('returning to the app checks after a minute and avoids repeated requests', () async {
    var now = DateTime(2026, 11, 1);
    var requests = 0;
    final u = Updater.forTesting(
      now: () => now,
      online: () => true,
      buildNumber: () async => 79,
      automaticChecks: () async => true,
      findUpdate: (_) async {
        requests++;
        return null;
      },
    );
    await u.check();
    now = now.add(const Duration(seconds: 30));
    await u.check(onResume: true);
    expect(requests, 1);
    now = now.add(const Duration(seconds: 31));
    await u.check(onResume: true);
    expect(requests, 2);
    await u.check(force: true);
    expect(requests, 3);
  });

  test('automatic checking can be disabled while explicit checks still work', () async {
    var requests = 0;
    final u = Updater.forTesting(
      online: () => true,
      buildNumber: () async => 79,
      automaticChecks: () async => false,
      findUpdate: (_) async {
        requests++;
        return release;
      },
    );
    await u.check();
    expect(requests, 0);
    expect(u.lastChecked.value, isNull);
    await u.check(force: true);
    expect(requests, 1);
    expect(u.available.value, release);
  });

  test('a manual check waits for an automatic check that is already running', () async {
    final found = Completer<UpdateInfo?>();
    var requests = 0;
    var manualFinished = false;
    final u = Updater.forTesting(
      online: () => true,
      buildNumber: () async => 79,
      automaticChecks: () async => true,
      findUpdate: (_) {
        requests++;
        return found.future;
      },
    );
    final automatic = u.check();
    final manual = u.check(force: true).then((_) => manualFinished = true);
    await Future<void>.value();
    expect(manualFinished, isFalse);
    expect(u.checking.value, isTrue);
    found.complete(release);
    await Future.wait([automatic, manual]);
    expect(requests, 1);
    expect(u.checking.value, isFalse);
    expect(u.available.value, release);
  });

  test('a joined explicit check overrides the automatic-check preference', () async {
    final preference = Completer<bool>();
    var requests = 0;
    final u = Updater.forTesting(
      online: () => true,
      buildNumber: () async => 79,
      automaticChecks: () => preference.future,
      findUpdate: (_) async {
        requests++;
        return release;
      },
    );
    final automatic = u.check();
    final manual = u.check(force: true);
    preference.complete(false);
    await Future.wait([automatic, manual]);
    expect(requests, 1);
    expect(u.available.value, release);
  });

  test('an offline check explains why it could not check, without claiming up to date', () async {
    var requests = 0;
    final u = Updater.forTesting(
        online: () => false,
        findUpdate: (_) async {
          requests++;
          return null;
        });
    await u.check(force: true);
    expect(requests, 0);
    expect(u.lastChecked.value, isNull);
    expect(u.error.value, contains('not connected'));
    expect(u.checking.value, isFalse);
  });

  testWidgets('a failed check retries next minute instead of waiting for the successful-check cooldown',
      (tester) async {
    var now = DateTime(2026, 11, 1);
    var fail = false;
    var requests = 0;
    final u = Updater.forTesting(
      now: () => now,
      online: () => true,
      buildNumber: () async => 79,
      automaticChecks: () async => true,
      findUpdate: (_) async {
        requests++;
        if (fail) throw StateError('offline');
        return null;
      },
    );
    await u.check();
    fail = true;
    final failed = u.check(force: true);
    await tester.pump();
    await tester.pump(const Duration(seconds: 10));
    await tester.pump(const Duration(seconds: 20));
    await failed;
    expect(u.error.value, isNotNull);
    expect(requests, 4);
    fail = false;
    now = now.add(const Duration(seconds: 30));
    await u.check();
    expect(requests, 4);
    now = now.add(const Duration(seconds: 31));
    await u.check();
    expect(requests, 5);
    expect(u.error.value, isNull);
  });

  testWidgets('late progress cannot restore downloading after the installer opens', (tester) async {
    final cache = Directory.systemTemp.createTempSync('updater_late_');
    addTearDown(() => cache.deleteSync(recursive: true));
    final finished = Completer<void>();
    final lateProgress = Completer<double>();
    var opened = 0;
    final u = Updater.forTesting(
      cacheDirectory: () async => cache,
      download: ({required url, required path}) async {
        await finished.future;
        File(path)
          ..parent.createSync(recursive: true)
          ..writeAsStringSync('test APK');
      },
      readProgress: () => lateProgress.future,
      openInstaller: (_) async {
        opened++;
        return 'started';
      },
    )..available.value = release;
    final install = u.install();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    finished.complete();
    await tester.pump();
    await install;
    expect(opened, 1);
    expect(u.progress.value, isNull);
    lateProgress.complete(1);
    await tester.pump();
    expect(u.progress.value, isNull);
    expect(u.installing.value, isFalse);
    await u.install();
    expect(opened, 2);
  });

  test('rapid taps open one installer even before cache lookup completes', () async {
    final cache = Directory.systemTemp.createTempSync('updater_taps_');
    addTearDown(() => cache.deleteSync(recursive: true));
    File('${cache.path}/updates/sanctum-80.apk')
      ..parent.createSync(recursive: true)
      ..writeAsStringSync('test APK');
    final directory = Completer<Directory>();
    var opened = 0;
    final u = Updater.forTesting(
      cacheDirectory: () => directory.future,
      openInstaller: (_) async {
        opened++;
        return 'started';
      },
    )..available.value = release;
    final first = u.install();
    final second = u.install();
    directory.complete(cache);
    await Future.wait([first, second]);
    expect(opened, 1);
  });

  test('cache preparation failures release the buttons for another attempt', () async {
    final u = Updater.forTesting(cacheDirectory: () async => throw FileSystemException('full'))
      ..available.value = release;
    await u.install();
    expect(u.installing.value, isFalse);
    expect(u.progress.value, isNull);
    expect(u.error.value, contains('free space'));
  });

  for (final scale in [1.0, 2.0]) {
    testWidgets('check and install stay on the Updates page at text scale $scale', (tester) async {
      tester.view.physicalSize = const Size(320, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      FlutterSecureStorage.setMockInitialValues({});
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          const MethodChannel('sanctum/background'), (call) async => call.method == 'buildNumber' ? 79 : null);
      addTearDown(() => tester.binding.defaultBinaryMessenger
          .setMockMethodCallHandler(const MethodChannel('sanctum/background'), null));
      final cache = Directory.systemTemp.createTempSync('updater_page_');
      addTearDown(() => cache.deleteSync(recursive: true));
      var found = false;
      var opened = 0;
      final u = Updater.forTesting(
        online: () => true,
        buildNumber: () async => 79,
        automaticChecks: () async => true,
        findUpdate: (_) async => found ? release : null,
        cacheDirectory: () async => cache,
        download: ({required url, required path}) async {
          File(path)
            ..parent.createSync(recursive: true)
            ..writeAsStringSync('test APK');
        },
        openInstaller: (_) async {
          opened++;
          return opened == 1 ? 'permission' : 'started';
        },
      );
      await tester.pumpWidget(RepaintBoundary(
          key: _shotKey,
          child: MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: sanctumTheme(),
            builder: (context, child) =>
                MediaQuery(data: MediaQuery.of(context).copyWith(textScaler: TextScaler.linear(scale)), child: child!),
            home: Scaffold(
                appBar: AppBar(title: const Text('Updates')),
                body: SingleChildScrollView(child: UpdateSettings(updater: u))),
          )));
      await tester.pumpAndSettle();
      expect(find.text('Sanctum is up to date.'), findsOneWidget);
      found = true;
      await tester.ensureVisible(find.text('Check for updates'));
      await tester.tap(find.text('Check for updates'));
      await tester.pumpAndSettle();
      expect(find.text('Update now'), findsOneWidget);
      expect(find.text('Sanctum beta-1.0.0.80 is available'), findsOneWidget);
      await _shot(tester, scale == 1 ? 'updates-normal' : 'updates-test-200-percent');
      await tester.ensureVisible(find.text('Update now'));
      await tester.tap(find.text('Update now'));
      await tester.pumpAndSettle();
      expect(opened, 1);
      expect(find.text('Allow Sanctum to install apps, then tap Update again.'), findsOneWidget);
      await tester.ensureVisible(find.text('Update now'));
      await tester.tap(find.text('Update now'));
      await tester.pumpAndSettle();
      expect(opened, 2);
      expect(find.text('Updates'), findsOneWidget);
      expect(find.textContaining('home screen'), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    });
  }
}
