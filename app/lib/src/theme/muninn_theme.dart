/// Maps a Muninn theme (the same `crates/weirdnet-themes/themes/*.toml` the TUI uses, resolved by Rust)
/// to Flutter's [ThemeData] plus the [MuninnColors] extension for Muninn-only roles.
library;

import 'package:flutter/material.dart';

/// Parses `#rrggbb` into an opaque [Color].
Color hexColor(String hex) {
  final h = hex.startsWith('#') ? hex.substring(1) : hex;
  return Color(0xFF000000 | int.parse(h, radix: 16));
}

/// Every Muninn colour role, with the TUI's names.
@immutable
class MuninnColors extends ThemeExtension<MuninnColors> {
  const MuninnColors({
    required this.bg,
    required this.fg,
    required this.muted,
    required this.border,
    required this.borderFocus,
    required this.selectionBg,
    required this.selectionFg,
    required this.accent,
    required this.header,
    required this.up,
    required this.down,
    required this.warning,
    required this.error,
    required this.success,
    required this.info,
    required this.chart,
  });

  final Color bg, fg, muted, border, borderFocus, selectionBg, selectionFg, accent, header;
  final Color up, down, warning, error, success, info;
  final List<Color> chart;

  /// Builds from resolved roles (`role -> #rrggbb`); missing roles fall back sensibly.
  factory MuninnColors.fromRoles(Map<String, String> r) {
    Color c(String k, String fallback) => hexColor(r[k] ?? r[fallback] ?? '#808080');
    return MuninnColors(
      bg: c('bg', 'bg'),
      fg: c('fg', 'fg'),
      muted: c('muted', 'fg'),
      border: c('border', 'muted'),
      borderFocus: c('border_focus', 'accent'),
      selectionBg: c('selection_bg', 'border'),
      selectionFg: c('selection_fg', 'fg'),
      accent: c('accent', 'header'),
      header: c('header', 'accent'),
      up: c('up', 'success'),
      down: c('down', 'error'),
      warning: c('warning', 'accent'),
      error: c('error', 'down'),
      success: c('success', 'up'),
      info: c('info', 'accent'),
      chart: [for (var i = 1; i <= 6; i++) c('chart_$i', 'accent')],
    );
  }

  /// `#rrggbb` for SVG substitution (the logo).
  static String hex(Color c) =>
      '#${(c.toARGB32() & 0xFFFFFF).toRadixString(16).padLeft(6, '0')}';

  @override
  MuninnColors copyWith({Color? header, Color? accent, Color? bg}) => MuninnColors(
        bg: bg ?? this.bg,
        fg: fg,
        muted: muted,
        border: border,
        borderFocus: borderFocus,
        selectionBg: selectionBg,
        selectionFg: selectionFg,
        accent: accent ?? this.accent,
        header: header ?? this.header,
        up: up,
        down: down,
        warning: warning,
        error: error,
        success: success,
        info: info,
        chart: chart,
      );

  @override
  MuninnColors lerp(ThemeExtension<MuninnColors>? other, double t) {
    if (other is! MuninnColors) return this;
    Color l(Color a, Color b) => Color.lerp(a, b, t)!;
    return MuninnColors(
      bg: l(bg, other.bg),
      fg: l(fg, other.fg),
      muted: l(muted, other.muted),
      border: l(border, other.border),
      borderFocus: l(borderFocus, other.borderFocus),
      selectionBg: l(selectionBg, other.selectionBg),
      selectionFg: l(selectionFg, other.selectionFg),
      accent: l(accent, other.accent),
      header: l(header, other.header),
      up: l(up, other.up),
      down: l(down, other.down),
      warning: l(warning, other.warning),
      error: l(error, other.error),
      success: l(success, other.success),
      info: l(info, other.info),
      chart: [for (var i = 0; i < chart.length; i++) l(chart[i], other.chart[i])],
    );
  }
}

/// The Flutter theme for one Muninn theme variant.
ThemeData muninnThemeData(Map<String, String> roles, {required bool dark}) {
  final m = MuninnColors.fromRoles(roles);
  final scheme = ColorScheme(
    brightness: dark ? Brightness.dark : Brightness.light,
    primary: m.header,
    onPrimary: m.bg,
    secondary: m.accent,
    onSecondary: m.bg,
    tertiary: m.info,
    onTertiary: m.bg,
    error: m.error,
    onError: m.bg,
    surface: m.bg,
    onSurface: m.fg,
    onSurfaceVariant: m.muted,
    outline: m.border,
    outlineVariant: m.border,
    primaryContainer: m.selectionBg,
    onPrimaryContainer: m.selectionFg,
    secondaryContainer: m.selectionBg,
    onSecondaryContainer: m.selectionFg,
  );
  return ThemeData(
    useMaterial3: true,
    colorScheme: scheme,
    scaffoldBackgroundColor: m.bg,
    dividerColor: m.border,
    extensions: [m],
    cardTheme: CardThemeData(
      color: m.bg,
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(6),
        side: BorderSide(color: m.border),
      ),
    ),
  );
}

extension MuninnThemeContext on BuildContext {
  /// The Muninn roles; derived from the ColorScheme if the extension isn't in scope.
  MuninnColors get muninn {
    final t = Theme.of(this);
    final m = t.extension<MuninnColors>();
    if (m != null) return m;
    final s = t.colorScheme;
    String h(Color c) => MuninnColors.hex(c);
    return MuninnColors.fromRoles({
      'bg': h(s.surface), 'fg': h(s.onSurface), 'muted': h(s.onSurfaceVariant), 'border': h(s.outline),
      'accent': h(s.secondary), 'header': h(s.primary), 'error': h(s.error),
      'selection_bg': h(s.primaryContainer), 'selection_fg': h(s.onPrimaryContainer),
    });
  }
}
