import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sanctum/chat/chat_screen.dart';
import 'package:sanctum/chat/message_source.dart';
import 'package:sanctum/deck/chat_card.dart';
import 'package:sanctum/home/home_screen.dart';
import 'package:sanctum/net/net_bus.dart';
import 'package:sanctum/src/rust/api/identity.dart';
import 'package:sanctum/src/rust/api/session.dart';
import 'package:sanctum/src/rust/api/sigil.dart';
import 'package:sanctum/src/rust/api/store.dart';
import 'package:sanctum/src/rust/frb_generated.dart';
import 'package:sanctum/theme/sigil_theme.dart';
import 'home_test.dart' show loadFonts, settle;

class _Messages implements MessageSource {
  @override
  bool get replies => true;
  @override
  Future<List<ChatMessage>> load({int? limit}) async => [
        ChatMessage(body: 'Still waiting', fromMe: true, sentAt: DateTime.now(), seq: 1),
        ChatMessage(body: 'Already arrived', fromMe: true, sentAt: DateTime.now(), seq: 2, delivered: true),
      ];
  @override
  Future<void> send(String body, {int? replyTo}) async {}
  @override
  Future<void> edit(int seq, String body) async {}
}

void main() {
  late SigilSpec sigil;
  late Uint8List key;
  setUpAll(() async {
    await RustLib.init(externalLibrary: ExternalLibrary.open('../target/debug/librust_lib_sanctum.so'));
    FlutterSecureStorage.setMockInitialValues({});
    final secret = await identityNew();
    key = await identityPublicKey(secret: secret);
    sigil = await sigilForKey(publicKey: key);
    await sessionOpen(
        identitySecret: secret,
        dir: Directory.systemTemp.createTempSync('privacy_ui_').path,
        storeKey: await storeNewKey());
    await loadFonts();
  });

  testWidgets('online, verified and delivered remain separate facts at large text', (tester) async {
    tester.view.physicalSize = const Size(320, 740);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final card = ChatCard(id: 10, title: 'Asha', sigil: sigil, source: _Messages(), friend: true, friendChatId: 10);
    await tester.pumpWidget(MaterialApp(
      theme: sanctumTheme(),
      builder: (context, child) =>
          MediaQuery(data: MediaQuery.of(context).copyWith(textScaler: const TextScaler.linear(2)), child: child!),
      home: ChatScreen(card: card, close: () {}),
    ));
    await settle(tester);
    expect(find.text('Queued on this phone'), findsOneWidget);
    expect(find.text('Delivered'), findsOneWidget);
    expect(find.text('Identity not verified'), findsOneWidget);
    NetBus.instance.online.value = {10};
    await settle(tester);
    expect(find.text('Sending'), findsOneWidget);
    expect(find.text('Identity not verified'), findsOneWidget);
    expect(find.text('Delivered'), findsOneWidget);
    await tester.longPress(find.text('Still waiting'));
    await settle(tester);
    expect(find.text('Only this phone. Others keep their copies.'), findsOneWidget);
    expect(find.text('Other phones are updated when reached. Saved copies can remain.'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.tapAt(const Offset(10, 10));
    await settle(tester);
    await tester.tap(find.byTooltip('More'));
    await settle(tester);
    await tester.tap(find.text('Chat privacy'));
    await settle(tester);
    expect(find.text('Connected privately online'), findsOneWidget);
    expect(find.text('Identity not verified'), findsWidgets);
    NetBus.instance.nearby.value = {10};
    await settle(tester);
    expect(find.text('Using nearby connection'), findsOneWidget);
    expect(tester.takeException(), isNull);
    NetBus.instance.nearby.value = {};
    NetBus.instance.online.value = {};
    await tester.pumpWidget(const SizedBox());
    await settle(tester, 2);
  });

  testWidgets('first-friend guidance fits a narrow phone with large text', (tester) async {
    tester.view.physicalSize = const Size(320, 740);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(MaterialApp(
      theme: sanctumTheme(),
      builder: (context, child) =>
          MediaQuery(data: MediaQuery.of(context).copyWith(textScaler: const TextScaler.linear(2)), child: child!),
      home: HomeScreen(publicKey: key, mySigil: sigil),
    ));
    await settle(tester, 20);
    expect(find.text('Your circle starts with one friend'), findsOneWidget);
    expect(find.text('Add a friend'), findsOneWidget);
    final target = tester.getSize(find.byTooltip('Settings'));
    expect(target.width, greaterThanOrEqualTo(48));
    expect(target.height, greaterThanOrEqualTo(48));
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
    await settle(tester, 2);
  });
}
