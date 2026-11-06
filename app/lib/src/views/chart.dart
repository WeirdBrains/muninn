/// A price chart in the theme's colours: candles when they fit, else a close line, with
/// volume underneath. Bars arrive already downsampled by Rust.
///
/// Pinch, the mouse wheel or a trackpad zooms inside the selected time range (the picker
/// is not changed). Drag pans when zoomed; double-tap or Reset zoom shows everything
/// again. Layers match the TUI's chart layers (`L`) for what this chart already draws.
library;

import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';

/// Hovering (or touching / dragging on a phone) shows that bar's values in the header row.
class BarChart extends ConsumerStatefulWidget {
  const BarChart({super.key, required this.bars, this.layers = const ChartLayers()});
  final List<BarPoint> bars;
  final ChartLayers layers;
  @override
  ConsumerState<BarChart> createState() => _BarChartState();
}

/// The one-line value readout for a bar (also used by tests).
String barReadout(BarPoint b) {
  String n(double v) => v.toStringAsPrecision(6);
  return '${fmtUtc(b.ts)} UTC  O ${n(b.open)}  H ${n(b.high)}  L ${n(b.low)}  C ${n(b.close)}  V ${n(b.volume)}';
}

class _BarChartState extends ConsumerState<BarChart> {
  int? _hover;
  double _baseScale = 1;
  int _baseStart = 0;
  Offset _baseFocal = Offset.zero;

  List<BarPoint> get _all => widget.bars;

  ChartViewport get _view => ref.read(chartViewportProvider);

  int get _visibleCount => _view.visibleCount(_all.length);

  int get _visibleStart => _view.visibleStart(_all.length);

  List<BarPoint> get _visible {
    final n = _all.length;
    if (n == 0) return const [];
    final start = _visibleStart;
    return _all.sublist(start, math.min(n, start + _visibleCount));
  }

  bool get _zoomed => _view.zoomed(_all.length);

  void _at(double dx, double width) {
    final bars = _visible;
    if (bars.isEmpty || width <= 0) return;
    final i = (dx / width * bars.length).floor().clamp(0, bars.length - 1);
    if (i != _hover) setState(() => _hover = i);
  }

  void _clear() {
    if (_hover != null) setState(() => _hover = null);
  }

  void _zoomTo(double newScale, double focalX, double width) {
    final n = _all.length;
    if (n == 0 || width <= 0) return;
    final frac = (focalX / width).clamp(0.0, 1.0);
    ref.read(chartViewportProvider.notifier).state = _view.zoomAt(newScale, n: n, frac: frac);
  }

  void _resetZoom() {
    if (_view.scale == 1 && _view.start == 0) return;
    ref.read(chartViewportProvider.notifier).state = const ChartViewport();
  }

  @override
  void didUpdateWidget(BarChart old) {
    super.didUpdateWidget(old);
    if (!listEquals(old.bars, widget.bars)) {
      _hover = null;
      ref.read(chartViewportProvider.notifier).state = const ChartViewport();
    }
  }

  @override
  Widget build(BuildContext context) {
    ref.watch(chartViewportProvider);
    final m = context.muninn;
    final layers = widget.layers;
    final all = _all;
    if (all.isEmpty) {
      return Center(child: Text('No bars in this time range', style: TextStyle(color: m.muted)));
    }
    final bars = _visible;
    final hi = bars.map((b) => b.high).reduce(math.max);
    final lo = bars.map((b) => b.low).reduce(math.min);
    final label = TextStyle(color: m.muted, fontSize: 11);
    final h = _hover == null || _hover! >= bars.length ? null : bars[_hover!];
    final countLabel = _zoomed ? '${bars.length} of ${all.length} bars' : '${all.length} bars';
    return Semantics(
      label: 'Price chart, $countLabel. Pinch or scroll to zoom, drag to pan when zoomed.',
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        h == null || !layers.crosshair
            ? Row(children: [
                if (layers.priceAxis) Text(hi.toStringAsPrecision(6), style: label),
                const Spacer(),
                Text(countLabel, key: const ValueKey('chart-count'), style: label),
                if (_zoomed)
                  TextButton(
                    key: const ValueKey('chart-reset-zoom'),
                    onPressed: _resetZoom,
                    child: const Text('Reset zoom'),
                  ),
              ])
            : Text(
                barReadout(h),
                key: const ValueKey('bar-readout'),
                style: label.copyWith(color: h.close >= h.open ? m.up : m.down),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
        Expanded(
          child: LayoutBuilder(
            builder: (_, box) => MouseRegion(
              onHover: (e) => _at(e.localPosition.dx, box.maxWidth),
              onExit: (_) => _clear(),
              child: Listener(
                onPointerSignal: (e) {
                  if (e is PointerScrollEvent && e.scrollDelta.dy != 0) {
                    final factor = e.scrollDelta.dy < 0 ? 1.15 : 1 / 1.15;
                    _zoomTo(_view.scale * factor, e.localPosition.dx, box.maxWidth);
                  }
                },
                child: GestureDetector(
                  onDoubleTap: _resetZoom,
                  onScaleStart: (d) {
                    _baseScale = _view.scale;
                    _baseStart = _visibleStart;
                    _baseFocal = d.localFocalPoint;
                    _at(d.localFocalPoint.dx, box.maxWidth);
                  },
                  onScaleUpdate: (d) {
                    final width = box.maxWidth;
                    if (d.pointerCount > 1 || (d.scale - 1).abs() > 0.02) {
                      _zoomTo(_baseScale * d.scale, d.localFocalPoint.dx, width);
                    } else if (_view.scale > 1.001) {
                      final vis = math.max(1, (all.length / _baseScale).round());
                      final newStart = (_baseStart - ((d.localFocalPoint.dx - _baseFocal.dx) / width * vis)).round();
                      ref.read(chartViewportProvider.notifier).state =
                          ChartViewport(scale: _view.scale, start: newStart).clampTo(all.length);
                    }
                    _at(d.localFocalPoint.dx, width);
                  },
                  child: CustomPaint(
                    painter: BarChartPainter(bars, m, layers: layers, hover: layers.crosshair ? _hover : null),
                    size: Size.infinite,
                  ),
                ),
              ),
            ),
          ),
        ),
        if (layers.priceAxis)
          Row(children: [
            Text(lo.toStringAsPrecision(6), style: label),
            const Spacer(),
          ]),
        if (layers.timeAxis)
          Row(children: [
            Flexible(child: Text(fmtUtc(bars.first.ts), style: label, overflow: TextOverflow.ellipsis)),
            const Spacer(),
            Flexible(
              child: Text('${fmtUtc(bars.last.ts)} UTC', style: label, overflow: TextOverflow.ellipsis, textAlign: TextAlign.end),
            ),
          ]),
      ]),
    );
  }
}

class BarChartPainter extends CustomPainter {
  BarChartPainter(this.bars, this.c, {this.layers = const ChartLayers(), this.hover});
  final List<BarPoint> bars;
  final MuninnColors c;
  final ChartLayers layers;

  /// The bar under the pointer: a vertical guide is drawn through it.
  final int? hover;

  /// Candles need at least this many pixels each.
  static const minCandle = 3.0;

  bool candlesFit(double width) => bars.isNotEmpty && width / bars.length >= minCandle;

  @override
  void paint(Canvas canvas, Size size) {
    if (bars.isEmpty || size.isEmpty) return;
    final volOn = layers.volume;
    final priceH = volOn ? size.height * 0.8 : size.height;
    final volTop = priceH + 4;
    final volH = size.height - volTop;
    final hi = bars.map((b) => b.high).reduce(math.max);
    final lo = bars.map((b) => b.low).reduce(math.min);
    final span = (hi - lo) == 0 ? 1.0 : hi - lo;
    final vmax = bars.map((b) => b.volume).fold<double>(0, math.max);
    double y(double p) => priceH - (p - lo) / span * priceH;
    final w = size.width / bars.length;
    if (layers.grid) {
      final grid = Paint()
        ..color = c.border
        ..strokeWidth = 1;
      for (var i = 0; i <= 4; i++) {
        final gy = priceH * i / 4;
        canvas.drawLine(Offset(0, gy), Offset(size.width, gy), grid);
      }
    }
    final up = Paint()..color = c.up;
    final down = Paint()..color = c.down;
    final drawCandles = layers.candles && candlesFit(size.width);
    final drawOhlc = layers.ohlc && !drawCandles;
    final fallbackLine = layers.candles && !candlesFit(size.width) && !layers.ohlc && !layers.line;
    final drawLine = layers.line || fallbackLine;

    if (layers.area && drawLine) {
      final path = Path();
      for (var i = 0; i < bars.length; i++) {
        final pt = Offset(i * w + w / 2, y(bars[i].close));
        i == 0 ? path.moveTo(pt.dx, pt.dy) : path.lineTo(pt.dx, pt.dy);
      }
      path.lineTo((bars.length - 1) * w + w / 2, priceH);
      path.lineTo(w / 2, priceH);
      path.close();
      canvas.drawPath(
        path,
        Paint()
          ..color = (c.chart.isEmpty ? c.accent : c.chart.first).withValues(alpha: 0.18)
          ..style = PaintingStyle.fill,
      );
    }

    if (volOn && vmax > 0 && volH > 2) {
      for (var i = 0; i < bars.length; i++) {
        final b = bars[i];
        final h = b.volume / vmax * volH;
        final p = Paint()..color = (b.close >= b.open ? c.up : c.down).withValues(alpha: 0.55);
        canvas.drawRect(Rect.fromLTWH(i * w, size.height - h, math.max(w - 1, 1), h), p);
      }
    }

    if (drawCandles) {
      for (var i = 0; i < bars.length; i++) {
        final b = bars[i];
        final x = i * w + w / 2;
        final p = b.close >= b.open ? up : down;
        canvas.drawLine(Offset(x, y(b.high)), Offset(x, y(b.low)), p..strokeWidth = 1);
        final top = y(math.max(b.open, b.close)), bot = y(math.min(b.open, b.close));
        canvas.drawRect(Rect.fromLTRB(x - w * 0.35, top, x + w * 0.35, math.max(bot, top + 1)), p);
      }
    } else if (drawOhlc) {
      for (var i = 0; i < bars.length; i++) {
        final b = bars[i];
        final x = i * w + w / 2;
        final p = (b.close >= b.open ? up : down)..strokeWidth = 1;
        canvas.drawLine(Offset(x, y(b.high)), Offset(x, y(b.low)), p);
        canvas.drawLine(Offset(x - w * 0.35, y(b.open)), Offset(x, y(b.open)), p);
        canvas.drawLine(Offset(x, y(b.close)), Offset(x + w * 0.35, y(b.close)), p);
      }
    }

    if (drawLine) {
      final path = Path();
      for (var i = 0; i < bars.length; i++) {
        final pt = Offset(i * w + w / 2, y(bars[i].close));
        i == 0 ? path.moveTo(pt.dx, pt.dy) : path.lineTo(pt.dx, pt.dy);
      }
      canvas.drawPath(
        path,
        Paint()
          ..color = c.chart.isEmpty ? c.accent : c.chart.first
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5,
      );
    }

    final hv = hover;
    if (layers.crosshair && hv != null && hv >= 0 && hv < bars.length) {
      final x = hv * w + w / 2;
      canvas.drawLine(
        Offset(x, 0),
        Offset(x, size.height),
        Paint()
          ..color = c.muted
          ..strokeWidth = 1,
      );
    }
  }

  @override
  bool shouldRepaint(BarChartPainter old) =>
      old.bars != bars || old.c != c || old.hover != hover || old.layers != layers;
}

/// The TUI's Chart layers sheet (`L` / View ▸ Chart layers…).
Future<void> showChartLayersSheet(BuildContext context, WidgetRef ref) => showModalBottomSheet(
      context: context,
      showDragHandle: true,
      builder: (ctx) => Consumer(builder: (ctx, ref, _) {
        final layers = ref.watch(chartLayersProvider);
        final m = ctx.muninn;
        Widget tile(String title, bool value, ChartLayers Function(bool) next) => SwitchListTile(
              minTileHeight: 56,
              title: Text(title),
              value: value,
              activeThumbColor: m.accent,
              onChanged: (v) => ref.read(chartLayersProvider.notifier).state = next(v),
            );
        return SafeArea(
          child: ListView(shrinkWrap: true, children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
              child: Text('Chart layers', style: TextStyle(color: m.header, fontWeight: FontWeight.w800, fontSize: 16)),
            ),
            tile('Candles', layers.candles, (v) => layers.copyWith(candles: v)),
            tile('OHLC bars', layers.ohlc, (v) => layers.copyWith(ohlc: v)),
            tile('Line', layers.line, (v) => layers.copyWith(line: v)),
            tile('Area under the line', layers.area, (v) => layers.copyWith(area: v)),
            tile('Volume', layers.volume, (v) => layers.copyWith(volume: v)),
            tile('Grid', layers.grid, (v) => layers.copyWith(grid: v)),
            tile('Crosshair + value readout', layers.crosshair, (v) => layers.copyWith(crosshair: v)),
            tile('Price axis', layers.priceAxis, (v) => layers.copyWith(priceAxis: v)),
            tile('Time axis', layers.timeAxis, (v) => layers.copyWith(timeAxis: v)),
          ]),
        );
      }),
    );

/// The list form of the same bars (View ▸ Change view).
class BarTable extends StatelessWidget {
  const BarTable({super.key, required this.bars});
  final List<BarPoint> bars;

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    String n(double v) => v.toStringAsPrecision(6);
    return ListView.builder(
      itemCount: bars.length,
      itemExtent: 32,
      itemBuilder: (_, i) {
        final b = bars[bars.length - 1 - i];
        return Row(children: [
          SizedBox(width: 140, child: Text(fmtUtc(b.ts), style: TextStyle(color: m.muted))),
          for (final v in [b.open, b.high, b.low]) Expanded(child: Text(n(v))),
          Expanded(child: Text(n(b.close), style: TextStyle(color: b.close >= b.open ? m.up : m.down))),
          Expanded(child: Text(n(b.volume), style: TextStyle(color: m.muted))),
        ]);
      },
    );
  }
}
