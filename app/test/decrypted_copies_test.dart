import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sanctum/chat/media.dart';
import 'package:sanctum/security/panic.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Directory cache, support;

  setUp(() {
    cache = Directory.systemTemp.createTempSync('sanctum_cache');
    support = Directory.systemTemp.createTempSync('sanctum_support');
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(const MethodChannel('plugins.flutter.io/path_provider'), (call) async {
      return switch (call.method) {
        'getApplicationCacheDirectory' => cache.path,
        'getApplicationSupportDirectory' => support.path,
        _ => null,
      };
    });
  });

  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(const MethodChannel('plugins.flutter.io/path_provider'), null);
    cache.deleteSync(recursive: true);
    support.deleteSync(recursive: true);
  });

  test('an open-once video is copied where every start and the emergency wipe clean up', () async {
    final copy = await decryptedCopyFile('once-1');
    expect(copy.parent.path, '${cache.path}/open');
    copy.writeAsStringSync('plain video');
    expect(await panicWipePaths(), contains('${cache.path}/open'));

    await clearDecryptedCopies();
    expect(copy.existsSync(), isFalse);
  });

  test('copies older versions left in the cache folder are cleaned up too', () async {
    final legacy = File('${cache.path}/once-1700000000000000')..writeAsStringSync('plain video');
    final opened = File('${cache.path}/open/7-photo.jpg')
      ..createSync(recursive: true)
      ..writeAsStringSync('plain photo');
    final picked = File('${cache.path}/outgoing/pick.jpg')
      ..createSync(recursive: true)
      ..writeAsStringSync('plain pick');
    final other = File('${cache.path}/unrelated')..writeAsStringSync('keep');
    final tor = Directory('${cache.path}/tor-cache')..createSync();

    expect(await panicWipePaths(), containsAll([legacy.path, '${cache.path}/open', '${cache.path}/outgoing']));

    await clearDecryptedCopies();
    expect(legacy.existsSync(), isFalse);
    expect(opened.existsSync(), isFalse);
    expect(picked.existsSync(), isFalse);
    expect(other.existsSync(), isTrue, reason: 'only plain copies go at start');
    expect(tor.existsSync(), isTrue);
  });
}
