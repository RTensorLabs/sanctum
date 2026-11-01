import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sanctum/security/panic.dart';

void main() {
  for (final reduced in [false, true]) {
    testWidgets('wipe needs two uninterrupted seconds, reduced motion: $reduced', (tester) async {
      final previous = debugSemanticsDisableAnimations;
      debugSemanticsDisableAnimations = reduced;
      addTearDown(() => debugSemanticsDisableAnimations = previous);
      var wipes = 0;
      await tester.pumpWidget(MaterialApp(home: Scaffold(body: HoldToWipe(onWipe: () => wipes++))));
      final press = await tester.startGesture(tester.getCenter(find.byType(HoldToWipe)));
      await tester.pump(const Duration(milliseconds: 150));
      expect(wipes, 0);
      await tester.pump(const Duration(milliseconds: 1849));
      expect(wipes, 0);
      await tester.pump(const Duration(milliseconds: 1));
      expect(wipes, 1);
      await tester.pump(const Duration(seconds: 2));
      expect(wipes, 1);
      await press.up();
    });
  }
  testWidgets('releasing or cancelling never carries time into the next hold', (tester) async {
    var wipes = 0;
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: HoldToWipe(onWipe: () => wipes++))));
    var press = await tester.startGesture(tester.getCenter(find.byType(HoldToWipe)));
    await tester.pump(const Duration(milliseconds: 1500));
    await press.up();
    press = await tester.startGesture(tester.getCenter(find.byType(HoldToWipe)));
    await tester.pump(const Duration(milliseconds: 1500));
    expect(wipes, 0);
    await press.cancel();
    await tester.pump(const Duration(seconds: 3));
    expect(wipes, 0);
  });
  testWidgets('moving a finger or leaving the app cancels the hold', (tester) async {
    var wipes = 0;
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: HoldToWipe(onWipe: () => wipes++))));
    var press = await tester.startGesture(tester.getCenter(find.byType(HoldToWipe)));
    await tester.pump(const Duration(seconds: 1));
    await press.moveBy(const Offset(40, 0));
    await tester.pump(const Duration(seconds: 2));
    expect(wipes, 0);
    await press.up();
    press = await tester.startGesture(tester.getCenter(find.byType(HoldToWipe)));
    await tester.pump(const Duration(seconds: 1));
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    await tester.pump(const Duration(seconds: 2));
    expect(wipes, 0);
    await press.up();
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
  });
}
