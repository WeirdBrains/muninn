import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/theme/muninn_theme.dart';
import 'package:muninn/src/window_actions.dart';
import 'package:muninn/src/window_chrome.dart';
import 'package:muninn/src/window_controls.dart';

const _roles = {
  'bg': '#1a1b26',
  'fg': '#c0caf5',
  'muted': '#565f89',
  'border': '#3b4261',
  'border_focus': '#7aa2f7',
  'selection_bg': '#283457',
  'selection_fg': '#c0caf5',
  'accent': '#7aa2f7',
  'header': '#bb9af7',
  'up': '#9ece6a',
  'down': '#f7768e',
  'warning': '#e0af68',
  'error': '#f7768e',
  'success': '#9ece6a',
  'info': '#7dcfff',
};

Widget _harness({
  required WindowChromePolicy policy,
  required WindowActions actions,
}) {
  return MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: muninnThemeData(_roles, dark: true),
    home: Scaffold(
      body: RepaintBoundary(
        key: const ValueKey('shot'),
        child: ColoredBox(
          color: hexColor(_roles['bg']!),
          child: SizedBox(
            height: 72,
            width: 800,
            child: MuninnTitleBar(
              policy: policy,
              actions: actions,
              child: const Align(
                alignment: Alignment.centerLeft,
                child: Padding(
                  padding: EdgeInsets.symmetric(horizontal: 12),
                  child: Text('MUNINN', style: TextStyle(fontSize: 22)),
                ),
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

void main() {
  group('WindowChromePolicy.resolve', () {
    test('Android and iOS have no custom chrome', () {
      for (final p in [TargetPlatform.android, TargetPlatform.iOS]) {
        final policy = WindowChromePolicy.resolve(platform: p);
        expect(policy.enabled, isFalse);
        expect(policy.showClose, isFalse);
        expect(policy.showMinimize, isFalse);
        expect(policy.showMaximize, isFalse);
      }
    });

    test('Windows and non-tiling Linux put all controls on the right', () {
      for (final env in <Map<String, String>>[
        {},
        {'XDG_CURRENT_DESKTOP': 'GNOME'},
        {'XDG_CURRENT_DESKTOP': 'KDE'},
        {'XDG_CURRENT_DESKTOP': 'ubuntu:GNOME'},
        {'XDG_CURRENT_DESKTOP': 'XFCE'},
        {'DESKTOP_SESSION': 'cinnamon'},
      ]) {
        for (final p in [TargetPlatform.windows, TargetPlatform.linux]) {
          if (p == TargetPlatform.windows && env.isNotEmpty) continue;
          final policy = WindowChromePolicy.resolve(
            platform: p,
            environment: env,
          );
          expect(policy.enabled, isTrue, reason: '$p $env');
          expect(policy.side, WindowControlSide.right, reason: '$p $env');
          expect(policy.showMinimize, isTrue, reason: '$p $env');
          expect(policy.showMaximize, isTrue, reason: '$p $env');
          expect(policy.showClose, isTrue, reason: '$p $env');
          expect(policy.doubleClickMaximizes, isTrue, reason: '$p $env');
          expect(policy.allowDrag, isTrue);
        }
      }
    });

    test('macOS puts traffic-light controls on the left', () {
      final policy = WindowChromePolicy.resolve(platform: TargetPlatform.macOS);
      expect(policy.enabled, isTrue);
      expect(policy.side, WindowControlSide.left);
      expect(policy.showMinimize, isTrue);
      expect(policy.showMaximize, isTrue);
      expect(policy.showClose, isTrue);
      expect(policy.doubleClickMaximizes, isTrue);
    });

    test('Linux tiling desktops show only close', () {
      const cases = <Map<String, String>>[
        {'XDG_CURRENT_DESKTOP': 'Hyprland'},
        {'XDG_CURRENT_DESKTOP': 'sway'},
        {'XDG_CURRENT_DESKTOP': 'i3'},
        {'XDG_CURRENT_DESKTOP': 'sway:wlroots'},
        {'XDG_SESSION_DESKTOP': 'i3-gaps'},
        {'DESKTOP_SESSION': 'awesome'},
        {'XDG_CURRENT_DESKTOP': 'river'},
        {'XDG_CURRENT_DESKTOP': 'niri'},
        {'XDG_CURRENT_DESKTOP': 'bspwm'},
        {'HYPRLAND_INSTANCE_SIGNATURE': 'abc123'},
        {'SWAYSOCK': '/run/user/1000/sway-ipc.sock'},
        {'I3SOCK': '/run/user/1000/i3/ipc.sock'},
      ];
      for (final env in cases) {
        final policy = WindowChromePolicy.resolve(
          platform: TargetPlatform.linux,
          environment: env,
        );
        expect(policy.enabled, isTrue, reason: '$env');
        expect(policy.side, WindowControlSide.right, reason: '$env');
        expect(policy.showClose, isTrue, reason: '$env');
        expect(policy.showMinimize, isFalse, reason: '$env');
        expect(policy.showMaximize, isFalse, reason: '$env');
        expect(policy.doubleClickMaximizes, isFalse, reason: '$env');
        expect(policy.allowDrag, isTrue, reason: '$env');
      }
    });

    test(
      'tiling detection does not fire on macOS or Windows env leftovers',
      () {
        final env = {'XDG_CURRENT_DESKTOP': 'Hyprland'};
        final mac = WindowChromePolicy.resolve(
          platform: TargetPlatform.macOS,
          environment: env,
        );
        expect(mac.showMinimize, isTrue);
        expect(mac.side, WindowControlSide.left);
        final win = WindowChromePolicy.resolve(
          platform: TargetPlatform.windows,
          environment: env,
        );
        expect(win.showMinimize, isTrue);
        expect(win.side, WindowControlSide.right);
      },
    );

    test('isLinuxTilingWindowManager is case-insensitive', () {
      expect(
        isLinuxTilingWindowManager({'XDG_CURRENT_DESKTOP': 'HYPRLAND'}),
        isTrue,
      );
      expect(isLinuxTilingWindowManager({'DESKTOP_SESSION': 'Sway'}), isTrue);
      expect(
        isLinuxTilingWindowManager({'XDG_CURRENT_DESKTOP': 'Omarchy'}),
        isFalse,
      );
    });
  });

  group('MuninnTitleBar', () {
    testWidgets('Windows/Linux: min, max, close on the right of the title', (
      t,
    ) async {
      final actions = RecordingWindowActions();
      final policy = WindowChromePolicy.resolve(
        platform: TargetPlatform.windows,
      );
      await t.pumpWidget(_harness(policy: policy, actions: actions));
      final brand = t.getTopLeft(find.text('MUNINN'));
      final min = t.getTopLeft(find.byKey(windowControlMinimizeKey));
      final max = t.getTopLeft(find.byKey(windowControlMaximizeKey));
      final close = t.getTopLeft(find.byKey(windowControlCloseKey));
      expect(brand.dx, lessThan(min.dx));
      expect(min.dx, lessThan(max.dx));
      expect(max.dx, lessThan(close.dx));
      expect(find.byKey(windowControlRestoreKey), findsNothing);
    });

    testWidgets('macOS: close, min, max on the left of the title', (t) async {
      final actions = RecordingWindowActions();
      final policy = WindowChromePolicy.resolve(platform: TargetPlatform.macOS);
      await t.pumpWidget(_harness(policy: policy, actions: actions));
      final close = t.getTopLeft(find.byKey(windowControlCloseKey));
      final min = t.getTopLeft(find.byKey(windowControlMinimizeKey));
      final max = t.getTopLeft(find.byKey(windowControlMaximizeKey));
      final brand = t.getTopLeft(find.text('MUNINN'));
      expect(close.dx, lessThan(min.dx));
      expect(min.dx, lessThan(max.dx));
      expect(max.dx, lessThan(brand.dx));
    });

    testWidgets('Linux tiling: only close, on the right', (t) async {
      final actions = RecordingWindowActions();
      final policy = WindowChromePolicy.resolve(
        platform: TargetPlatform.linux,
        environment: const {'XDG_CURRENT_DESKTOP': 'Hyprland'},
      );
      await t.pumpWidget(_harness(policy: policy, actions: actions));
      expect(find.byKey(windowControlCloseKey), findsOneWidget);
      expect(find.byKey(windowControlMinimizeKey), findsNothing);
      expect(find.byKey(windowControlMaximizeKey), findsNothing);
      final brand = t.getTopLeft(find.text('MUNINN'));
      final close = t.getTopLeft(find.byKey(windowControlCloseKey));
      expect(brand.dx, lessThan(close.dx));
    });

    testWidgets('disabled policy draws no controls', (t) async {
      await t.pumpWidget(
        _harness(
          policy: WindowChromePolicy.disabled,
          actions: RecordingWindowActions(),
        ),
      );
      expect(find.byKey(windowControlCloseKey), findsNothing);
      expect(find.byKey(windowControlMinimizeKey), findsNothing);
      expect(find.byKey(windowControlMaximizeKey), findsNothing);
    });

    testWidgets('buttons call minimize, maximize and close', (t) async {
      final actions = RecordingWindowActions();
      final policy = WindowChromePolicy.resolve(
        platform: TargetPlatform.windows,
      );
      await t.pumpWidget(_harness(policy: policy, actions: actions));
      await t.tap(find.byKey(windowControlMinimizeKey));
      await t.tap(find.byKey(windowControlMaximizeKey));
      await t.pump();
      expect(actions.calls, ['minimize', 'toggleMaximize']);
      expect(actions.isMaximized, isTrue);
      expect(find.byKey(windowControlRestoreKey), findsOneWidget);
      await t.tap(find.byKey(windowControlRestoreKey));
      await t.tap(find.byKey(windowControlCloseKey));
      await t.pump();
      expect(actions.calls, [
        'minimize',
        'toggleMaximize',
        'toggleMaximize',
        'close',
      ]);
      expect(actions.isMaximized, isFalse);
    });

    testWidgets('double-click on the title toggles maximize when allowed', (
      t,
    ) async {
      final actions = RecordingWindowActions();
      final policy = WindowChromePolicy.resolve(
        platform: TargetPlatform.windows,
      );
      await t.pumpWidget(_harness(policy: policy, actions: actions));
      await t.tap(find.text('MUNINN'));
      await t.pump(kDoubleTapMinTime);
      await t.tap(find.text('MUNINN'));
      await t.pump();
      await t.pump(kDoubleTapTimeout);
      expect(actions.calls, contains('toggleMaximize'));
    });

    testWidgets('double-click does not maximize under a tiling WM', (t) async {
      final actions = RecordingWindowActions();
      final policy = WindowChromePolicy.resolve(
        platform: TargetPlatform.linux,
        environment: const {'XDG_CURRENT_DESKTOP': 'sway'},
      );
      await t.pumpWidget(_harness(policy: policy, actions: actions));
      await t.tap(find.text('MUNINN'));
      await t.pump(kDoubleTapMinTime);
      await t.tap(find.text('MUNINN'));
      await t.pump();
      await t.pump(kDoubleTapTimeout);
      expect(actions.calls, isNot(contains('toggleMaximize')));
    });

    testWidgets('dragging the title starts a window move', (t) async {
      final actions = RecordingWindowActions();
      final policy = WindowChromePolicy.resolve(platform: TargetPlatform.linux);
      await t.pumpWidget(_harness(policy: policy, actions: actions));
      await t.drag(find.text('MUNINN'), const Offset(40, 0));
      await t.pump();
      await t.pump(kDoubleTapTimeout);
      expect(actions.calls, contains('startDragging'));
    });
  });
}
