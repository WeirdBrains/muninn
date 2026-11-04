import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/logo.dart';
import 'package:muninn/src/theme/muninn_theme.dart';

import 'fake_api.dart';

void main() {
  test('hexColor parses #rrggbb', () {
    expect(hexColor('#7aa2f7'), const Color(0xFF7AA2F7));
    expect(MuninnColors.hex(const Color(0xFF1A1B26)), '#1a1b26');
  });

  test('roles map to ColorScheme and MuninnColors', () {
    final t = muninnThemeData(tokyoDark, dark: true);
    expect(t.colorScheme.brightness, Brightness.dark);
    expect(t.colorScheme.primary, hexColor('#7aa2f7'));
    expect(t.colorScheme.secondary, hexColor('#bb9af7'));
    expect(t.colorScheme.surface, hexColor('#1a1b26'));
    final m = t.extension<MuninnColors>()!;
    expect(m.up, hexColor('#9ece6a'));
    expect(m.chart, hasLength(6));
    expect(m.chart[3], hexColor('#ff9e64'));
  });

  test('missing roles fall back', () {
    final m = MuninnColors.fromRoles(nordLight);
    expect(m.borderFocus, hexColor('#b48ead')); // accent
    expect(m.info, hexColor('#b48ead'));
    expect(muninnThemeData(nordLight, dark: false).colorScheme.brightness, Brightness.light);
  });

  test('raven SVG takes the theme colours', () {
    const svg = '<path fill="var(--muninn-header, #7aa2f7)"/><path fill="var(--muninn-accent, #bb9af7)"/><rect fill="var(--muninn-bg, #1a1b26)"/>';
    final out = themeRaven(svg, MuninnColors.fromRoles(nordLight));
    expect(out, '<path fill="#5e81ac"/><path fill="#b48ead"/><rect fill="#eceff4"/>');
  });
}
