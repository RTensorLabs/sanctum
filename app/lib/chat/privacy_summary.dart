import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// Connectivity, identity and delivery are separate facts, never a blanket
/// promise that a conversation cannot be observed.
class PrivacyFact extends StatelessWidget {
  const PrivacyFact({super.key, required this.icon, required this.title, required this.detail});
  final IconData icon;
  final String title;
  final String detail;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.symmetric(vertical: Space.md),
        child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Icon(icon, color: Palette.textMuted),
          const SizedBox(width: Space.md),
          Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(title, style: AppText.bodyStrong),
            const SizedBox(height: Space.xs),
            Text(detail, style: AppText.caption),
          ])),
        ]),
      );
}

String deliveryLabel(
        {required bool delivered,
        required bool online,
        bool stored = true,
        bool group = false,
        bool temporary = false}) =>
    delivered
        ? (group ? 'Delivered to group' : 'Delivered')
        : !stored
            ? 'Preparing'
            : group
                ? 'Awaiting group delivery'
                : online
                    ? 'Sending'
                    : temporary
                        ? 'Waiting for your friend'
                        : 'Queued on this phone';
