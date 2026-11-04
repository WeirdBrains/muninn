/// The approved raven, themed: the SVG's `var(--muninn-header|accent|bg, …)` fills are
/// replaced with the active theme's colours (the website uses the same three roles).
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart' show rootBundle;
import 'package:flutter_svg/flutter_svg.dart';

import 'theme/muninn_theme.dart';

enum RavenVariant { tile, mark, small }

final _cache = <RavenVariant, Future<String>>{};

Future<String> _load(RavenVariant v) => _cache.putIfAbsent(v, () {
      final f = switch (v) {
        RavenVariant.tile => 'muninn-raven.svg',
        RavenVariant.mark => 'muninn-raven-mark.svg',
        RavenVariant.small => 'muninn-raven-small.svg',
      };
      return rootBundle.loadString('assets/logo/$f');
    });

/// Substitutes the three theme roles into the raven SVG.
String themeRaven(String svg, MuninnColors c) => svg
    .replaceAll(RegExp(r'var\(--muninn-header[^)]*\)'), MuninnColors.hex(c.header))
    .replaceAll(RegExp(r'var\(--muninn-accent[^)]*\)'), MuninnColors.hex(c.accent))
    .replaceAll(RegExp(r'var\(--muninn-bg[^)]*\)'), MuninnColors.hex(c.bg));

class RavenLogo extends StatelessWidget {
  const RavenLogo({super.key, this.size = 48, this.variant = RavenVariant.mark});
  final double size;
  final RavenVariant variant;

  @override
  Widget build(BuildContext context) {
    final c = context.muninn;
    return SizedBox.square(
      dimension: size,
      child: FutureBuilder<String>(
        future: _load(size < 32 ? RavenVariant.small : variant),
        builder: (context, snap) => snap.hasData
            ? SvgPicture.string(themeRaven(snap.data!, c), width: size, height: size, semanticsLabel: 'Muninn')
            : const SizedBox.shrink(),
      ),
    );
  }
}
