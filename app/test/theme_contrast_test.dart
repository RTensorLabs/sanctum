import 'dart:math' as math;

import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sanctum/theme/tokens.dart';

double contrast(Color foreground, Color background) {
  final painted = Color.alphaBlend(foreground, background).computeLuminance();
  final behind = background.computeLuminance();
  return (math.max(painted, behind) + 0.05) / (math.min(painted, behind) + 0.05);
}

void main() {
  test('message text and timestamps meet body-text contrast across accent colours', () {
    final colors = [
      ...Palette.accents,
      const Color(0xFF179BDE), // The light-mode bubble from the visual review.
      const Color(0xFF000000),
      const Color(0xFFFFFFFF),
      for (var hue = 0.0; hue < 360; hue += 15)
        for (final lightness in [0.25, 0.4, 0.48, 0.6, 0.75]) HSLColor.fromAHSL(1, hue, 0.7, lightness).toColor(),
    ];
    for (final background in colors) {
      expect(contrast(Palette.onColor(background), background), greaterThanOrEqualTo(4.5), reason: '$background body');
      expect(
        contrast(Palette.secondaryOnColor(background), background),
        greaterThanOrEqualTo(4.5),
        reason: '$background timestamp',
      );
      final quoted = Color.alphaBlend(Palette.onColor(background).withValues(alpha: 0.08), background);
      expect(contrast(Palette.secondaryOnColor(quoted), quoted), greaterThanOrEqualTo(4.5),
          reason: '$background quote');
    }
  });
}
