import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

import '../background/background.dart';
import '../identity/secrets.dart';
import '../src/rust/api/session.dart';
import '../theme/tokens.dart';

/// Erases Sanctum from this phone and closes it. The next start is a fresh
/// install: new identity, no friends, no chats.
///
/// Keys go first (crypto-shredding: the encrypted database is unreadable
/// without them, whatever flash storage keeps), then the files, then the
/// process with every temp chat in its memory.
Future<void> panicWipe() async {
  await wipeSecrets();
  await Background.stop();
  await Background.cancelAll();
  try {
    final support = (await getApplicationSupportDirectory()).path;
    final cache = (await getApplicationCacheDirectory()).path;
    await wipeEverything(paths: [
      for (final suffix in ['', '-wal', '-shm', '-journal']) '$support/sanctum.db$suffix',
      '$support/tor-state',
      '$support/media',
      '$cache/tor-cache',
      '$cache/open',
      '$cache/outgoing',
      '$cache/updates',
    ]);
  } catch (_) {
    // Already unreadable without the keys; leftovers are noise.
  }
  exit(0);
}

/// Press and hold to wipe: impossible to trigger by accident, quick on
/// purpose.
class HoldToWipe extends StatefulWidget {
  const HoldToWipe({super.key, this.onWipe});

  /// Optional action for an embedded hold control; otherwise wipes this phone.
  final VoidCallback? onWipe;

  @override
  State<HoldToWipe> createState() => _HoldToWipeState();
}

class _HoldToWipeState extends State<HoldToWipe> with SingleTickerProviderStateMixin, WidgetsBindingObserver {
  static const _duration = Duration(seconds: 2);
  late final _hold =
      AnimationController(vsync: this, duration: _duration, animationBehavior: AnimationBehavior.preserve);
  Timer? _deadline;
  int? _pointer;
  Offset? _origin;
  bool _fired = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  void _cancel() {
    _deadline?.cancel();
    _deadline = null;
    _pointer = null;
    _origin = null;
    if (!_fired) _hold.reset();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) _cancel();
  }

  void _start(PointerDownEvent event) {
    if (_pointer != null || _fired) return;
    _pointer = event.pointer;
    _origin = event.position;
    HapticFeedback.mediumImpact();
    _hold.forward(from: 0);
    // Real elapsed time controls deletion; animation speed never does.
    _deadline = Timer(_duration, () {
      if (!mounted || _pointer == null || ModalRoute.of(context)?.isCurrent == false) {
        _cancel();
        return;
      }
      _fired = true;
      HapticFeedback.heavyImpact();
      if (widget.onWipe case final action?) {
        action();
      } else {
        unawaited(panicWipe());
      }
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _deadline?.cancel();
    _hold.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Listener(
      onPointerDown: _start,
      onPointerUp: (event) {
        if (event.pointer == _pointer) _cancel();
      },
      onPointerCancel: (event) {
        if (event.pointer == _pointer) _cancel();
      },
      onPointerMove: (event) {
        final box = context.findRenderObject() as RenderBox;
        if (event.pointer == _pointer &&
            (!(Offset.zero & box.size).contains(box.globalToLocal(event.position)) ||
                (event.position - _origin!).distance > kTouchSlop)) {
          _cancel();
        }
      },
      child: AnimatedBuilder(
        animation: _hold,
        builder: (context, _) => Container(
          height: 52,
          clipBehavior: Clip.antiAlias,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(Radii.md),
            border: Border.all(color: Palette.danger.withValues(alpha: 0.7)),
          ),
          child: Stack(
            fit: StackFit.expand,
            children: [
              FractionallySizedBox(
                alignment: Alignment.centerLeft,
                widthFactor: _hold.value,
                child: const ColoredBox(color: Palette.danger),
              ),
              Center(
                child: Text(
                  _hold.value == 0 ? 'Hold to erase this phone' : 'Keep holding…',
                  style: AppText.bodyStrong.copyWith(color: _hold.value > 0.5 ? Colors.white : Palette.danger),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// What a panic wipe does, with the hold button: the sheet from Settings,
/// and the panel the lock screen draws itself (it sits above the app's
/// screens, so it can't open a sheet there).
class PanicPanel extends StatelessWidget {
  const PanicPanel({super.key, this.onWipe});

  /// Passed on to [HoldToWipe] (tests); otherwise wipes this phone.
  final VoidCallback? onWipe;

  @override
  Widget build(BuildContext context) => Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Text('Erase Sanctum from this phone', style: AppText.title),
          const SizedBox(height: Space.sm),
          Text(
            'Erases your identity, friends, every chat and temp chat from this phone, '
            'and closes Sanctum. It can\'t be undone. Your friends keep their copies.',
            style: AppText.caption,
          ),
          const SizedBox(height: Space.lg),
          HoldToWipe(onWipe: onWipe),
        ],
      );
}

/// The panic sheet, from inside the app (Settings).
Future<void> showPanicSheet(BuildContext context) => showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      // Not const: the panel takes the current colours (see Restyler).
      builder: (context) => SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(Space.xl, 0, Space.xl, Space.xl),
          child: PanicPanel(),
        ),
      ),
    );
