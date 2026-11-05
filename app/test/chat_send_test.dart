import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sanctum/chat/chat_screen.dart';
import 'package:sanctum/chat/message_source.dart';
import 'package:sanctum/deck/chat_card.dart';
import 'package:sanctum/src/rust/api/identity.dart';
import 'package:sanctum/src/rust/api/session.dart';
import 'package:sanctum/src/rust/api/sigil.dart';
import 'package:sanctum/src/rust/api/store.dart';
import 'package:sanctum/src/rust/frb_generated.dart';
import 'package:sanctum/sigil/sigil_look.dart';
import 'package:sanctum/sigil/sigil_painter.dart';
import 'package:sanctum/theme/appearance.dart';
import 'package:sanctum/theme/sigil_theme.dart';

/// A chat whose stored copy of a sent message arrives a moment after it's
/// sent, like the real store.
class _FakeSource implements MessageSource {
  final stored = <ChatMessage>[];

  /// Sending fails (no connection, a full disk...).
  bool fail = false;

  @override
  bool get replies => true;

  @override
  Future<List<ChatMessage>> load({int? limit}) async {
    await Future<void>.delayed(const Duration(milliseconds: 20));
    return List.of(stored);
  }

  @override
  Future<void> send(String body, {int? replyTo}) async {
    await Future<void>.delayed(const Duration(milliseconds: 60));
    if (fail) throw Exception('disk full at /data/secret/path');
    stored.add(ChatMessage(body: body, fromMe: true, sentAt: DateTime.now(), seq: stored.length + 1, canEdit: true));
  }

  @override
  Future<void> edit(int seq, String body) async {
    final i = stored.indexWhere((m) => m.seq == seq);
    final m = stored[i];
    stored[i] = ChatMessage(body: body, fromMe: true, sentAt: m.sentAt, seq: seq, edited: true, canEdit: true);
  }
}

/// The State object of the bubble that shows [text].
State? _bubbleStateOf(WidgetTester tester, String text) {
  State? found;
  tester.element(find.text(text)).visitAncestorElements((e) {
    if (e is StatefulElement && e.state.runtimeType.toString() == '_BubbleState') {
      found = e.state;
      return false;
    }
    return true;
  });
  return found;
}

final _shotKey = GlobalKey();

/// Saves a PNG of the chat when SCREENSHOT_DIR is set.
Future<void> _shot(WidgetTester tester, String name) async {
  final dir = Platform.environment['SCREENSHOT_DIR'];
  if (dir == null) return;
  await tester.runAsync(() async {
    final boundary = _shotKey.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await boundary.toImage(pixelRatio: 1);
    final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
    File('$dir/$name.png').writeAsBytesSync(bytes!.buffer.asUint8List());
  });
}

// Runs the real Rust core on the host: `cargo build -p rust_lib_sanctum` first.
void main() {
  setUpAll(() async {
    await RustLib.init(externalLibrary: ExternalLibrary.open('../target/debug/librust_lib_sanctum.so'));
    FlutterSecureStorage.setMockInitialValues({});
    final dir = Directory.systemTemp.createTempSync('sanctum_chat_test');
    await sessionOpen(identitySecret: await identityNew(), dir: dir.path, storeKey: await storeNewKey());
  });

  testWidgets('a message you send keeps its bubble when the stored copy replaces it', (tester) async {
    tester.view.physicalSize = const Size(1080, 2340);
    tester.view.devicePixelRatio = 2.75;
    addTearDown(tester.view.reset);

    final source = _FakeSource();
    late SigilSpec sigil;
    await tester.runAsync(() async {
      sigil = await sigilForKey(publicKey: await identityPublicKey(secret: await identityNew()));
    });
    final card = ChatCard(id: 1, title: 'Friend', sigil: sigil, source: source);
    // The keyboard channel is poked when a reply starts.
    final calls = <String>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.textInput, (call) async {
      calls.add(call.method);
      return null;
    });
    addTearDown(() => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.textInput, null));

    await tester.pumpWidget(Restyler(
      builder: (_) => MaterialApp(
        theme: sanctumTheme(accent: sigil.look.accent.color),
        home: RepaintBoundary(key: _shotKey, child: ChatScreen(card: card, close: () {})),
      ),
    ));
    Future<void> settle([int frames = 8]) async {
      for (var i = 0; i < frames; i++) {
        await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 40)));
        await tester.pump(const Duration(milliseconds: 100));
      }
    }

    await settle();
    await tester.enterText(find.byType(TextField).last, 'hello there');
    await tester.pump();
    await tester.tap(find.byTooltip('Send'));
    // The optimistic copy, before the store has it.
    await tester.pump();
    final before = _bubbleStateOf(tester, 'hello there');
    expect(before, isNotNull);
    expect(source.stored, isEmpty);

    await settle();
    expect(source.stored, hasLength(1));
    final after = _bubbleStateOf(tester, 'hello there');
    expect(find.text('hello there'), findsOneWidget);
    expect(identical(before, after), isTrue, reason: 'the bubble must not be rebuilt (it would blink)');

    // Choosing "Reply" brings the keyboard up, even if the field still has
    // focus from before the keyboard was hidden.
    await tester.longPress(find.text('hello there'));
    await settle(4);
    calls.clear();
    await tester.tap(find.text('Reply'));
    await settle(8);
    expect(calls, contains('TextInput.show'));
    expect(find.textContaining('Replying to'), findsOneWidget);
    await tester.tap(find.byTooltip('Cancel reply'));
    await settle(4);

    // "Edit" puts the words in the box; saving changes the bubble, marked edited.
    await tester.longPress(find.text('hello there'));
    await settle(4);
    await tester.tap(find.text('Edit'));
    await settle(8);
    expect(find.text('Editing your message'), findsOneWidget);
    final field = tester.widget<TextField>(find.byType(TextField).last);
    expect(field.controller!.text, 'hello there');
    await _shot(tester, 'chat_editing');
    await tester.enterText(find.byType(TextField).last, 'hello again');
    await tester.pump();
    await tester.tap(find.byTooltip('Save'));
    await settle();
    expect(source.stored.single.body, 'hello again');
    expect(find.text('hello again'), findsOneWidget);
    expect(find.text('hello there'), findsNothing);
    expect(find.textContaining('edited'), findsOneWidget);
    expect(find.text('Editing your message'), findsNothing);
    await _shot(tester, 'chat_edited');
  });

  // [wide]: room for the recording bar in the test font (wider than the app's).
  Future<_FakeSource> showChat(WidgetTester tester, {bool wide = false}) async {
    tester.view.physicalSize = Size(wide ? 1600 : 1080, 2340);
    tester.view.devicePixelRatio = 2.75;
    addTearDown(tester.view.reset);
    final source = _FakeSource();
    late SigilSpec sigil;
    await tester.runAsync(() async {
      sigil = await sigilForKey(publicKey: await identityPublicKey(secret: await identityNew()));
    });
    final card = ChatCard(id: 1, title: 'Friend', sigil: sigil, source: source);
    await tester.pumpWidget(Restyler(
      builder: (_) => MaterialApp(
        theme: sanctumTheme(accent: sigil.look.accent.color),
        home: RepaintBoundary(key: _shotKey, child: ChatScreen(card: card, close: () {})),
      ),
    ));
    await settleFor(tester);
    return source;
  }

  testWidgets('a message that fails to send goes back in the box, with a plain note', (tester) async {
    final source = await showChat(tester);
    source.fail = true;
    await tester.enterText(find.byType(TextField).last, 'please arrive');
    await tester.pump();
    await tester.tap(find.byTooltip('Send'));
    await tester.pump();
    expect(tester.widget<TextField>(find.byType(TextField).last).controller!.text, isEmpty);
    await settleFor(tester);

    expect(tester.widget<TextField>(find.byType(TextField).last).controller!.text, 'please arrive');
    expect(find.text('Couldn\'t send. Try again.'), findsOneWidget);
    expect(find.textContaining('disk full'), findsNothing, reason: 'never raw error text');
    expect(find.text('please arrive'), findsOneWidget, reason: 'only in the box, not as a sent bubble');
    expect(source.stored, isEmpty);

    // Trying again (once the note has gone) sends it.
    await tester.pump(const Duration(seconds: 5));
    await settleFor(tester, 3);
    source.fail = false;
    await tester.tap(find.byTooltip('Send'));
    await settleFor(tester);
    expect(source.stored.single.body, 'please arrive');
    expect(tester.widget<TextField>(find.byType(TextField).last).controller!.text, isEmpty);
    await tester.pumpWidget(const SizedBox());
  });

  group('voice notes', () {
    late Completer<bool> mic;
    late Completer<bool> started;
    late List<String> calls;
    setUp(() {
      mic = Completer<bool>();
      started = Completer<bool>();
      calls = [];
    });

    void fakeRecorder(WidgetTester tester) {
      const channel = MethodChannel('sanctum/app');
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (call) async {
        calls.add(call.method);
        return switch (call.method) {
          'requestMicrophone' => mic.future,
          'recordStart' => started.future,
          'recordLevel' => 0,
          _ => null,
        };
      });
      addTearDown(() => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, null));
    }

    Finder micButton() => find.byIcon(Icons.mic_rounded);

    testWidgets('letting go while the microphone question is up never starts recording', (tester) async {
      await showChat(tester, wide: true);
      fakeRecorder(tester);
      final press = await tester.startGesture(tester.getCenter(micButton()));
      await tester.pump(const Duration(milliseconds: 700));
      await settleFor(tester, 2);
      expect(calls, ['requestMicrophone']);
      await press.up();
      await tester.pump();
      mic.complete(true);
      started.complete(true);
      await settleFor(tester, 3);
      expect(calls, isNot(contains('recordStart')));
      expect(find.text('‹ Slide to cancel'), findsNothing);
      await tester.pumpWidget(const SizedBox());
    });

    testWidgets('the phone taking the touch away for the question never starts recording', (tester) async {
      await showChat(tester, wide: true);
      fakeRecorder(tester);
      final press = await tester.startGesture(tester.getCenter(micButton()));
      await tester.pump(const Duration(milliseconds: 700));
      await settleFor(tester, 2);
      await press.cancel();
      await tester.pump();
      mic.complete(true);
      started.complete(true);
      await settleFor(tester, 3);
      expect(calls, isNot(contains('recordStart')));
      expect(find.text('‹ Slide to cancel'), findsNothing);
      await tester.pumpWidget(const SizedBox());
    });

    testWidgets('letting go while recording starts stops it at once', (tester) async {
      await showChat(tester, wide: true);
      fakeRecorder(tester);
      mic.complete(true);
      final press = await tester.startGesture(tester.getCenter(micButton()));
      await tester.pump(const Duration(milliseconds: 700));
      await settleFor(tester, 2);
      expect(calls, ['requestMicrophone', 'recordStart']);
      await press.up();
      await tester.pump();
      started.complete(true);
      await settleFor(tester, 3);
      expect(calls, ['requestMicrophone', 'recordStart', 'recordCancel']);
      expect(find.text('‹ Slide to cancel'), findsNothing);
      await tester.pumpWidget(const SizedBox());
    });

    testWidgets('holding records, letting go stops, and one press makes one recording', (tester) async {
      await showChat(tester, wide: true);
      fakeRecorder(tester);
      mic.complete(true);
      started.complete(true);
      final press = await tester.startGesture(tester.getCenter(micButton()));
      await tester.pump(const Duration(milliseconds: 700));
      await settleFor(tester, 2);
      expect(find.text('‹ Slide to cancel'), findsOneWidget);
      await tester.pump(const Duration(milliseconds: 500));
      final levels = calls.where((c) => c == 'recordLevel').length;
      expect(levels, greaterThan(0));
      await press.up();
      await settleFor(tester, 3);
      expect(calls.where((c) => c == 'recordStart'), hasLength(1));
      expect(calls, contains('recordStop'));
      expect(calls.where((c) => c == 'recordLevel'), hasLength(levels), reason: 'level checks stop with recording');
      expect(find.text('‹ Slide to cancel'), findsNothing);
      await tester.pumpWidget(const SizedBox());
    });
  });
}

Future<void> settleFor(WidgetTester tester, [int frames = 8]) async {
  for (var i = 0; i < frames; i++) {
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 40)));
    await tester.pump(const Duration(milliseconds: 100));
  }
}
