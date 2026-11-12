/// Themed custom window controls and a draggable title area.
library;

import 'package:flutter/material.dart';

import 'theme/muninn_theme.dart';
import 'window_actions.dart';
import 'window_chrome.dart';

const windowTitleBarKey = Key('window-title-bar');
const windowControlMinimizeKey = Key('window-control-minimize');
const windowControlMaximizeKey = Key('window-control-maximize');
const windowControlRestoreKey = Key('window-control-restore');
const windowControlCloseKey = Key('window-control-close');

/// Minimize, maximize/restore and close buttons, laid out for [policy].
class MuninnWindowControls extends StatelessWidget {
  const MuninnWindowControls({
    super.key,
    required this.policy,
    required this.actions,
    this.isMaximized = false,
  });

  final WindowChromePolicy policy;
  final WindowActions actions;
  final bool isMaximized;

  @override
  Widget build(BuildContext context) {
    if (!policy.enabled) return const SizedBox.shrink();
    final m = context.muninn;
    final mac = policy.side == WindowControlSide.left;
    final buttons = <Widget>[
      if (mac && policy.showClose)
        _MacLight(
          key: windowControlCloseKey,
          tooltip: 'Close',
          color: m.error,
          icon: Icons.close,
          onPressed: actions.close,
        ),
      if (mac && policy.showMinimize)
        _MacLight(
          key: windowControlMinimizeKey,
          tooltip: 'Minimize',
          color: m.warning,
          icon: Icons.remove,
          onPressed: actions.minimize,
        ),
      if (mac && policy.showMaximize)
        _MacLight(
          key: isMaximized ? windowControlRestoreKey : windowControlMaximizeKey,
          tooltip: isMaximized ? 'Restore' : 'Maximize',
          color: m.success,
          icon: isMaximized ? Icons.filter_none : Icons.crop_square,
          onPressed: actions.toggleMaximize,
        ),
      if (!mac && policy.showMinimize)
        _BarButton(
          key: windowControlMinimizeKey,
          tooltip: 'Minimize',
          icon: Icons.remove,
          onPressed: actions.minimize,
        ),
      if (!mac && policy.showMaximize)
        _BarButton(
          key: isMaximized ? windowControlRestoreKey : windowControlMaximizeKey,
          tooltip: isMaximized ? 'Restore' : 'Maximize',
          icon: isMaximized ? Icons.filter_none : Icons.crop_square,
          onPressed: actions.toggleMaximize,
        ),
      if (!mac && policy.showClose)
        _BarButton(
          key: windowControlCloseKey,
          tooltip: 'Close',
          icon: Icons.close,
          hoverBackground: m.error,
          hoverForeground: m.bg,
          onPressed: actions.close,
        ),
    ];
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (mac) const SizedBox(width: 4),
        ..._withGaps(buttons, gap: mac ? 8 : 0),
        if (mac) const SizedBox(width: 8),
      ],
    );
  }

  List<Widget> _withGaps(List<Widget> buttons, {required double gap}) {
    if (gap == 0 || buttons.length < 2) return buttons;
    final out = <Widget>[buttons.first];
    for (final b in buttons.skip(1)) {
      out.add(SizedBox(width: gap));
      out.add(b);
    }
    return out;
  }
}

/// Draggable title area with window controls on the platform's side.
///
/// [child] is the brand (and any other title-bar content). Double-click
/// toggles maximize when [WindowChromePolicy.doubleClickMaximizes] is set.
class MuninnTitleBar extends StatefulWidget {
  const MuninnTitleBar({
    super.key,
    required this.policy,
    required this.actions,
    required this.child,
  });

  final WindowChromePolicy policy;
  final WindowActions actions;
  final Widget child;

  @override
  State<MuninnTitleBar> createState() => _MuninnTitleBarState();
}

class _MuninnTitleBarState extends State<MuninnTitleBar> {
  late bool _maximized = widget.actions.isMaximized;

  void _sync() {
    final next = widget.actions.isMaximized;
    if (next != _maximized) setState(() => _maximized = next);
  }

  void _toggleMaximize() {
    if (!widget.policy.doubleClickMaximizes) return;
    widget.actions.toggleMaximize();
    _sync();
  }

  @override
  Widget build(BuildContext context) {
    final policy = widget.policy;
    final actions = widget.actions;
    final controls = MuninnWindowControls(
      policy: policy,
      actions: _SyncingActions(actions, onChanged: _sync),
      isMaximized: _maximized,
    );

    Widget dragChild = widget.child;
    if (policy.allowDrag || policy.doubleClickMaximizes) {
      dragChild = GestureDetector(
        behavior: HitTestBehavior.translucent,
        onPanStart: policy.allowDrag ? (_) => actions.startDragging() : null,
        onDoubleTap: policy.doubleClickMaximizes ? _toggleMaximize : null,
        child: widget.child,
      );
    }

    final row = Row(
      children: [
        if (policy.enabled && policy.side == WindowControlSide.left) controls,
        Expanded(child: dragChild),
        if (policy.enabled && policy.side == WindowControlSide.right) controls,
      ],
    );

    return KeyedSubtree(key: windowTitleBarKey, child: row);
  }
}

/// Forwards actions and notifies the title bar after maximize/restore.
class _SyncingActions implements WindowActions {
  const _SyncingActions(this.inner, {required this.onChanged});
  final WindowActions inner;
  final VoidCallback onChanged;

  @override
  void minimize() => inner.minimize();

  @override
  void toggleMaximize() {
    inner.toggleMaximize();
    onChanged();
  }

  @override
  void close() => inner.close();

  @override
  void startDragging() => inner.startDragging();

  @override
  bool get isMaximized => inner.isMaximized;
}

class _MacLight extends StatefulWidget {
  const _MacLight({
    super.key,
    required this.tooltip,
    required this.color,
    required this.icon,
    required this.onPressed,
  });

  final String tooltip;
  final Color color;
  final IconData icon;
  final VoidCallback onPressed;

  @override
  State<_MacLight> createState() => _MacLightState();
}

class _MacLightState extends State<_MacLight> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: widget.tooltip,
      child: MouseRegion(
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: GestureDetector(
          onTap: widget.onPressed,
          child: Container(
            width: 14,
            height: 14,
            decoration: BoxDecoration(
              color: widget.color,
              shape: BoxShape.circle,
            ),
            alignment: Alignment.center,
            child: _hover
                ? Icon(widget.icon, size: 9, color: const Color(0xD9000000))
                : const SizedBox.shrink(),
          ),
        ),
      ),
    );
  }
}

class _BarButton extends StatefulWidget {
  const _BarButton({
    super.key,
    required this.tooltip,
    required this.icon,
    required this.onPressed,
    this.hoverBackground,
    this.hoverForeground,
  });

  final String tooltip;
  final IconData icon;
  final VoidCallback onPressed;
  final Color? hoverBackground;
  final Color? hoverForeground;

  @override
  State<_BarButton> createState() => _BarButtonState();
}

class _BarButtonState extends State<_BarButton> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final bg = _hover ? (widget.hoverBackground ?? m.selectionBg) : null;
    final fg = _hover ? (widget.hoverForeground ?? m.fg) : m.muted;
    return Tooltip(
      message: widget.tooltip,
      child: MouseRegion(
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: GestureDetector(
          onTap: widget.onPressed,
          child: Container(
            width: 46,
            height: 36,
            color: bg,
            alignment: Alignment.center,
            child: Icon(widget.icon, size: 16, color: fg),
          ),
        ),
      ),
    );
  }
}
