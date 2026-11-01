import 'package:flutter/widgets.dart';

/// The Sigil design system: every spacing, radius, colour, type style and
/// motion value the app uses. Screens use these, never one-off numbers.
abstract final class Space {
  static const xs = 4.0;
  static const sm = 8.0;
  static const md = 12.0;
  static const lg = 16.0;
  static const xl = 24.0;
  static const xxl = 32.0;
}

abstract final class TouchTarget {
  static const minimum = 48.0;
}

abstract final class Radii {
  static const sm = 12.0;
  static const md = 16.0;
  static const lg = 22.0;
  static const xl = 28.0;
}

/// Neutral surfaces; colour comes from each chat's sigil accent. Dark or
/// light, as picked in Settings (see `Appearance`).
abstract final class Palette {
  /// Set by `Appearance`; screens rebuild when it changes.
  static bool light = false;

  // Light mode is warm paper rather than cold white, with warm ink.
  static Color get bg => light ? const Color(0xFFF7F2E9) : const Color(0xFF09090C);
  static Color get surface => light ? const Color(0xFFFFFCF6) : const Color(0xFF111116);
  static Color get surfaceHigh => light ? const Color(0xFFEFE7DA) : const Color(0xFF191920);
  static Color get outline => light ? const Color(0x1C3A2810) : const Color(0x14FFFFFF);
  static Color get outlineStrong => light ? const Color(0x2E3A2810) : const Color(0x24FFFFFF);
  static Color get text => light ? const Color(0xF01E160D) : const Color(0xF0FFFFFF);
  static Color get textMuted => light ? const Color(0xA61E160D) : const Color(0x9EFFFFFF);
  static Color get textFaint => light ? const Color(0xB31E160D) : const Color(0xA6FFFFFF);
  static const danger = Color(0xFFFF5D5D);

  /// Sanctum's own gold (the door in the logo), a little deeper in light mode.
  static Color get gold => light ? const Color(0xFFB8862F) : const Color(0xFFE8BE62);

  /// A person's colour in a group chat: the same for the same name on every
  /// screen, readable in light and dark.
  static Color person(String name) {
    var h = 0;
    for (final c in name.codeUnits) {
      h = (h * 31 + c) & 0x7fffffff;
    }
    final base = HSLColor.fromColor(accents[h % accents.length]);
    return (light ? base.withLightness((base.lightness * 0.62).clamp(0.0, 1.0)) : base).toColor();
  }

  static const warning = Color(0xFFFFC145);

  /// Accent colours to pick from in Settings, instead of the sigil's own.
  static const accents = [
    Color(0xFF8B7CFF),
    Color(0xFF5B9DFF),
    Color(0xFF3CC8C8),
    Color(0xFF4CD08A),
    Color(0xFFC6E356),
    Color(0xFFFFC145),
    Color(0xFFFF8A4C),
    Color(0xFFFF5D8F),
    Color(0xFFD86BFF),
  ];

  /// A chat card's colour: its sigil's own dark background, or in light
  /// mode a soft tint of its accent.
  static Color card(Color background, Color accent) =>
      light ? Color.alphaBlend(accent.withValues(alpha: 0.12), const Color(0xFFFFFCF6)) : background;

  /// Black or white, whichever has the higher text contrast.
  static Color onColor(Color background) =>
      background.computeLuminance() > 0.179 ? const Color(0xFF000000) : const Color(0xFFFFFFFF);

  /// Quieter text on an accent, kept readable even on mid-tone colours.
  static Color secondaryOnColor(Color background) {
    final foreground = onColor(background);
    final base = background.computeLuminance() + 0.05;
    for (var alpha = 0.7; alpha < 1; alpha += 0.05) {
      final color = Color.alphaBlend(foreground.withValues(alpha: alpha), background);
      final luminance = color.computeLuminance() + 0.05;
      final contrast = luminance > base ? luminance / base : base / luminance;
      if (contrast >= 4.5) return color;
    }
    return foreground;
  }
}

/// Artwork stays behind the conversation, especially on warm paper.
abstract final class DecorationOpacity {
  static double get chatSigil => Palette.light ? 0.04 : 0.07;
}

abstract final class AppText {
  // One warm, rounded family for everything but codes.
  static const _display = 'Figtree';
  static const _body = 'Figtree';
  static const _mono = 'JetBrainsMono';

  static const display =
      TextStyle(fontFamily: _display, fontSize: 30, fontWeight: FontWeight.w800, letterSpacing: -0.9, height: 1.1);
  static const title = TextStyle(fontFamily: _display, fontSize: 21, fontWeight: FontWeight.w800, letterSpacing: -0.5);
  static const heading =
      TextStyle(fontFamily: _display, fontSize: 17, fontWeight: FontWeight.w700, letterSpacing: -0.2);
  static const body = TextStyle(fontFamily: _body, fontSize: 16, fontWeight: FontWeight.w400, height: 1.35);
  static const bodyStrong = TextStyle(fontFamily: _body, fontSize: 15, fontWeight: FontWeight.w600);
  static TextStyle get caption =>
      TextStyle(fontFamily: _body, fontSize: 13.5, fontWeight: FontWeight.w500, color: Palette.textMuted);
  static TextStyle get micro =>
      TextStyle(fontFamily: _body, fontSize: 11.5, fontWeight: FontWeight.w500, color: Palette.textFaint);
  static TextStyle get overline => TextStyle(
      fontFamily: _body, fontSize: 11.5, fontWeight: FontWeight.w600, letterSpacing: 2.2, color: Palette.textFaint);
  static const mono =
      TextStyle(fontFamily: _mono, fontSize: 15, fontWeight: FontWeight.w500, letterSpacing: 1.2, height: 1.6);
}

/// Shared timings for purposeful motion, with the phone's reduced-motion preference.
abstract final class Motion {
  static const fast = Duration(milliseconds: 160);
  static const medium = Duration(milliseconds: 280);
  static const slow = Duration(milliseconds: 420);
  static const arrival = Duration(milliseconds: 180);
  static const navigation = Duration(milliseconds: 240);
  static const unread = Duration(milliseconds: 420);
  static bool enabled(BuildContext context) => !(MediaQuery.maybeOf(context)?.disableAnimations ??
      WidgetsBinding.instance.platformDispatcher.accessibilityFeatures.disableAnimations);
  static Duration duration(BuildContext context, Duration value) => enabled(context) ? value : Duration.zero;

  /// Repeating decoration must explicitly stop when the phone reduces motion.
  static void repeat(BuildContext context, AnimationController controller, {bool reverse = false}) {
    if (enabled(context)) {
      if (!controller.isAnimating) controller.repeat(reverse: reverse);
    } else {
      controller.stop();
      controller.value = 0;
    }
  }

  static const emphasized = Cubic(0.2, 0, 0, 1);
  static const decelerate = Cubic(0.05, 0.7, 0.1, 1);
  static const accelerate = Cubic(0.3, 0, 0.8, 0.15);
}
