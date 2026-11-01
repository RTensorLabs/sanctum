import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sanctum/sigil/sigil_painter.dart';
import 'package:sanctum/src/rust/api/sigil.dart';

void main() {
  testWidgets('decorative sigils stop and resume when the phone changes reduced motion', (tester) async {
    final reduced = ValueNotifier(false);
    addTearDown(reduced.dispose);
    const blue = HslColor(h: 210, s: 0.7, l: 0.5);
    const sigil = SigilSpec(
        accent: blue,
        palette: [blue, blue, blue],
        background: HslColor(h: 210, s: 0.2, l: 0.1),
        form: SigilForm.rosette,
        symmetry: 6,
        layers: 3,
        rotation: 0,
        density: 0.6,
        weight: 0.5,
        seed: 42);
    await tester.pumpWidget(MaterialApp(
        home: ValueListenableBuilder<bool>(
      valueListenable: reduced,
      builder: (context, disabled, _) => MediaQuery(
        data: MediaQuery.of(context).copyWith(disableAnimations: disabled),
        child: const SizedBox.square(dimension: 100, child: LivingSigil(spec: sigil)),
      ),
    )));
    await tester.pump(const Duration(seconds: 1));
    expect(tester.widget<RotationTransition>(find.byType(RotationTransition)).turns.value, greaterThan(0));
    reduced.value = true;
    await tester.pump();
    await tester.pump(const Duration(seconds: 1));
    expect(tester.widget<RotationTransition>(find.byType(RotationTransition)).turns.value, 0);
    reduced.value = false;
    await tester.pump();
    await tester.pump(const Duration(seconds: 1));
    expect(tester.widget<RotationTransition>(find.byType(RotationTransition)).turns.value, greaterThan(0));
    await tester.pumpWidget(const SizedBox());
  });
}
