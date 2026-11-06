/// A price chart in the theme's colours: candles when they fit, else a close line, with
/// volume underneath. Bars arrive already downsampled by Rust.
///
/// Pinch, the mouse wheel or a trackpad zooms inside the selected time range (the picker
/// is not changed). Drag pans when zoomed; double-tap or Reset zoom shows everything
/// again. Layers match the TUI's chart layers (`L`), including envelope, moving averages,
/// wick/gap/zero-volume markers.
library;

import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../workspace.dart';
import '../tiling_extras.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';

/// Hovering (or touching / dragging on a phone) shows that bar's values in the header row.
class BarChart extends ConsumerStatefulWidget {
  const BarChart({
    super.key,
    required this.bars,
    this.layers = const ChartLayers(),
    this.tileId,
  });
  final List<BarPoint> bars;
  final ChartLayers layers;
  final int? tileId;
  @override
  ConsumerState<BarChart> createState() => _BarChartState();
}

/// The one-line value readout for a bar (also used by tests).
String barReadout(BarPoint b) {
  String n(double v) => v.toStringAsPrecision(6);
  final zero = b.volume == 0 ? '  ∅' : '';
  return '${fmtUtc(b.ts)} UTC  O ${n(b.open)}  H ${n(b.high)}  L ${n(b.low)}  C ${n(b.close)}  V ${n(b.volume)}$zero';
}

/// TUI `sma`: simple moving average of the last [n] values. Earlier points are null.
List<double?> sma(List<double> v, int n) {
  final out = List<double?>.filled(v.length, null);
  if (n <= 0) return out;
  var sum = 0.0;
  for (var i = 0; i < v.length; i++) {
    sum += v[i];
    if (i >= n) sum -= v[i - n];
    if (i + 1 >= n) out[i] = sum / n;
  }
  return out;
}

/// TUI `ema`: k = 2/(n+1), seed at the first value, emit from index n-1.
List<double?> ema(List<double> v, int n) {
  final out = List<double?>.filled(v.length, null);
  if (n <= 0 || v.isEmpty) return out;
  final k = 2.0 / (n + 1);
  double? e;
  for (var i = 0; i < v.length; i++) {
    e = e == null ? v[i] : v[i] * k + e * (1 - k);
    if (i + 1 >= n) out[i] = e;
  }
  return out;
}

/// TUI shooting star: upper wick ≥ 2× body and ≥ 60% of the range, lower wick ≤ 25%.
bool shootingStar(BarPoint b) {
  final body = (b.close - b.open).abs();
  final upper = b.high - math.max(b.open, b.close);
  final lower = math.min(b.open, b.close) - b.low;
  final range = b.high - b.low;
  return range > 0 &&
      upper >= 2 * body &&
      upper >= 0.6 * range &&
      lower <= 0.25 * range;
}

/// 20-bar highest high / lowest low ending at [i], same window as the TUI envelope.
(double, double) envelopeAt(List<BarPoint> bars, int i) {
  final s = math.max(0, i - 19);
  var hi = bars[s].high;
  var lo = bars[s].low;
  for (var j = s + 1; j <= i; j++) {
    hi = math.max(hi, bars[j].high);
    lo = math.min(lo, bars[j].low);
  }
  return (hi, lo);
}

class _BarChartState extends ConsumerState<BarChart> {
  int? _hover;
  double _baseScale = 1;
  int _baseStart = 0;
  Offset _baseFocal = Offset.zero;

  List<BarPoint> get _all => widget.bars;

  ChartViewport get _view {
    final id = widget.tileId;
    if (id != null) {
      final t = ref.read(workspaceProvider).tile(id);
      if (t != null) return t.viewport;
    }
    return ref.read(chartViewportProvider);
  }

  void _setView(ChartViewport v) {
    final id = widget.tileId;
    if (id != null) {
      syncLinkedFromChart(ref, id, v, _all);
    } else {
      ref.read(chartViewportProvider.notifier).state = v;
    }
  }

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
    _setView(_view.zoomAt(newScale, n: n, frac: frac));
  }

  void _resetZoom() {
    if (_view.scale == 1 && _view.start == 0) return;
    _setView(const ChartViewport());
  }

  @override
  void didUpdateWidget(BarChart old) {
    super.didUpdateWidget(old);
    if (!listEquals(old.bars, widget.bars)) {
      _hover = null;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        _setView(const ChartViewport());
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    if (widget.tileId != null) {
      ref.watch(workspaceProvider);
    } else {
      ref.watch(chartViewportProvider);
    }
    final m = context.muninn;
    final layers = widget.layers;
    final mas = ref.watch(chartMasProvider);
    final metric = ref.watch(chartLineMetricProvider);
    final all = _all;
    if (all.isEmpty) {
      return Center(
        child: Text(
          'No bars in this time range',
          style: TextStyle(color: m.muted),
        ),
      );
    }
    final bars = _visible;
    final hi = bars.map((b) => b.high).reduce(math.max);
    final lo = bars.map((b) => b.low).reduce(math.min);
    final label = TextStyle(color: m.muted, fontSize: 11);
    final h = _hover == null || _hover! >= bars.length ? null : bars[_hover!];
    final countLabel = _zoomed
        ? '${bars.length} of ${all.length} bars'
        : '${all.length} bars';
    return Semantics(
      label:
          'Price chart, $countLabel. Pinch or scroll to zoom, drag to pan when zoomed.',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          h == null || !layers.crosshair
              ? Row(
                  children: [
                    if (layers.priceAxis)
                      Text(hi.toStringAsPrecision(6), style: label),
                    const Spacer(),
                    Text(
                      countLabel,
                      key: const ValueKey('chart-count'),
                      style: label,
                    ),
                    if (_zoomed)
                      TextButton(
                        key: const ValueKey('chart-reset-zoom'),
                        onPressed: _resetZoom,
                        child: const Text('Reset zoom'),
                      ),
                  ],
                )
              : Text(
                  barReadout(h),
                  key: const ValueKey('bar-readout'),
                  style: label.copyWith(
                    color: h.close >= h.open ? m.up : m.down,
                  ),
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
                      _zoomTo(
                        _view.scale * factor,
                        e.localPosition.dx,
                        box.maxWidth,
                      );
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
                        _zoomTo(
                          _baseScale * d.scale,
                          d.localFocalPoint.dx,
                          width,
                        );
                      } else if (_view.scale > 1.001) {
                        final vis = math.max(
                          1,
                          (all.length / _baseScale).round(),
                        );
                        final newStart =
                            (_baseStart -
                                    ((d.localFocalPoint.dx - _baseFocal.dx) /
                                        width *
                                        vis))
                                .round();
                        _setView(
                          ChartViewport(
                            scale: _view.scale,
                            start: newStart,
                          ).clampTo(all.length),
                        );
                      }
                      _at(d.localFocalPoint.dx, width);
                    },
                    child: CustomPaint(
                      painter: BarChartPainter(
                        bars,
                        m,
                        layers: layers,
                        hover: layers.crosshair ? _hover : null,
                        history: all,
                        visibleStart: _visibleStart,
                        mas: mas,
                        lineMetric: metric,
                      ),
                      size: Size.infinite,
                    ),
                  ),
                ),
              ),
            ),
          ),
          if (layers.priceAxis)
            Row(
              children: [
                Text(lo.toStringAsPrecision(6), style: label),
                const Spacer(),
              ],
            ),
          if (layers.timeAxis)
            Row(
              children: [
                Flexible(
                  child: Text(
                    fmtUtc(bars.first.ts),
                    style: label,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const Spacer(),
                Flexible(
                  child: Text(
                    '${fmtUtc(bars.last.ts)} UTC',
                    style: label,
                    overflow: TextOverflow.ellipsis,
                    textAlign: TextAlign.end,
                  ),
                ),
              ],
            ),
        ],
      ),
    );
  }
}

class BarChartPainter extends CustomPainter {
  BarChartPainter(
    this.bars,
    this.c, {
    this.layers = const ChartLayers(),
    this.hover,
    List<BarPoint>? history,
    this.visibleStart = 0,
    this.mas = defaultChartMas,
    this.lineMetric = ChartLineMetric.close,
  }) : history = history ?? bars;
  final List<BarPoint> bars;
  final List<BarPoint> history;
  final int visibleStart;
  final MuninnColors c;
  final ChartLayers layers;
  final List<ChartMa> mas;
  final ChartLineMetric lineMetric;

  /// The bar under the pointer: a vertical guide is drawn through it.
  final int? hover;

  /// Candles need at least this many pixels each.
  static const minCandle = 3.0;

  bool candlesFit(double width) =>
      bars.isNotEmpty && width / bars.length >= minCandle;

  Color _chart(int i) {
    if (c.chart.isEmpty) return c.accent;
    return c.chart[i % c.chart.length];
  }

  @override
  void paint(Canvas canvas, Size size) {
    if (bars.isEmpty || size.isEmpty) return;
    final volOn = layers.volume;
    final priceH = volOn ? size.height * 0.8 : size.height;
    final volTop = priceH + 4;
    final volH = size.height - volTop;
    var hi = bars.map((b) => b.high).reduce(math.max);
    var lo = bars.map((b) => b.low).reduce(math.min);
    final closes = [for (final b in history) b.close];
    final maSeries = layers.ma
        ? [
            for (final m in mas)
              m.ema ? ema(closes, m.period) : sma(closes, m.period),
          ]
        : const <List<double?>>[];
    if (layers.envelope) {
      for (var i = 0; i < bars.length; i++) {
        final (h, l) = envelopeAt(history, visibleStart + i);
        hi = math.max(hi, h);
        lo = math.min(lo, l);
      }
    }
    for (final series in maSeries) {
      for (var i = 0; i < bars.length; i++) {
        final idx = visibleStart + i;
        if (idx < series.length && series[idx] != null) {
          hi = math.max(hi, series[idx]!);
          lo = math.min(lo, series[idx]!);
        }
      }
    }
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
    final warn = Paint()..color = c.warning;
    final drawCandles = layers.candles && candlesFit(size.width);
    final drawOhlc = layers.ohlc && !drawCandles;
    final fallbackLine =
        layers.candles &&
        !candlesFit(size.width) &&
        !layers.ohlc &&
        !layers.line;
    final drawLine = layers.line || fallbackLine;

    if (layers.area && drawLine) {
      final path = Path();
      for (var i = 0; i < bars.length; i++) {
        final pt = Offset(i * w + w / 2, y(lineMetric.value(bars[i])));
        i == 0 ? path.moveTo(pt.dx, pt.dy) : path.lineTo(pt.dx, pt.dy);
      }
      path.lineTo((bars.length - 1) * w + w / 2, priceH);
      path.lineTo(w / 2, priceH);
      path.close();
      canvas.drawPath(
        path,
        Paint()
          ..color = _chart(0).withValues(alpha: 0.18)
          ..style = PaintingStyle.fill,
      );
    }

    if (volOn && vmax > 0 && volH > 2) {
      for (var i = 0; i < bars.length; i++) {
        final b = bars[i];
        final h = b.volume / vmax * volH;
        final p = Paint()
          ..color = (b.close >= b.open ? c.up : c.down).withValues(alpha: 0.55);
        canvas.drawRect(
          Rect.fromLTWH(i * w, size.height - h, math.max(w - 1, 1), h),
          p,
        );
        if (layers.zeroVolume && b.volume == 0) {
          canvas.drawCircle(
            Offset(i * w + w / 2, size.height - 3),
            3,
            warn
              ..style = PaintingStyle.stroke
              ..strokeWidth = 1.5,
          );
        }
      }
    }

    if (drawCandles) {
      for (var i = 0; i < bars.length; i++) {
        final b = bars[i];
        final x = i * w + w / 2;
        final p = b.close >= b.open ? up : down;
        final bodyTop = math.max(b.open, b.close);
        final bodyBot = math.min(b.open, b.close);
        final body = (b.close - b.open).abs();
        final upper = b.high - bodyTop;
        final wick = (layers.wicks && upper > body) ? warn : p;
        canvas.drawLine(
          Offset(x, y(b.high)),
          Offset(x, y(bodyTop)),
          wick..strokeWidth = 1,
        );
        canvas.drawLine(
          Offset(x, y(bodyBot)),
          Offset(x, y(b.low)),
          p..strokeWidth = 1,
        );
        final top = y(bodyTop), bot = y(bodyBot);
        canvas.drawRect(
          Rect.fromLTRB(
            x - w * 0.35,
            top,
            x + w * 0.35,
            math.max(bot, top + 1),
          ),
          p,
        );
      }
    } else if (drawOhlc) {
      for (var i = 0; i < bars.length; i++) {
        final b = bars[i];
        final x = i * w + w / 2;
        final p = (b.close >= b.open ? up : down)..strokeWidth = 1;
        canvas.drawLine(Offset(x, y(b.high)), Offset(x, y(b.low)), p);
        canvas.drawLine(
          Offset(x - w * 0.35, y(b.open)),
          Offset(x, y(b.open)),
          p,
        );
        canvas.drawLine(
          Offset(x, y(b.close)),
          Offset(x + w * 0.35, y(b.close)),
          p,
        );
      }
    }

    if (drawLine) {
      final path = Path();
      for (var i = 0; i < bars.length; i++) {
        final pt = Offset(i * w + w / 2, y(lineMetric.value(bars[i])));
        i == 0 ? path.moveTo(pt.dx, pt.dy) : path.lineTo(pt.dx, pt.dy);
      }
      canvas.drawPath(
        path,
        Paint()
          ..color = _chart(0)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5,
      );
    }

    if (layers.envelope) {
      final dash = Paint()
        ..color = _chart(2)
        ..strokeWidth = 1.5
        ..style = PaintingStyle.stroke
        ..strokeCap = StrokeCap.round;
      for (var i = 0; i < bars.length; i++) {
        final (h, l) = envelopeAt(history, visibleStart + i);
        final x = i * w + w / 2;
        final hw = math.max(1.0, w * 0.35);
        canvas.drawLine(Offset(x - hw, y(h)), Offset(x + hw, y(h)), dash);
        canvas.drawLine(Offset(x - hw, y(l)), Offset(x + hw, y(l)), dash);
      }
    }

    for (var k = 0; k < maSeries.length; k++) {
      final series = maSeries[k];
      final path = Path();
      var started = false;
      for (var i = 0; i < bars.length; i++) {
        final idx = visibleStart + i;
        if (idx >= series.length || series[idx] == null) {
          started = false;
          continue;
        }
        final pt = Offset(i * w + w / 2, y(series[idx]!));
        if (!started) {
          path.moveTo(pt.dx, pt.dy);
          started = true;
        } else {
          path.lineTo(pt.dx, pt.dy);
        }
      }
      canvas.drawPath(
        path,
        Paint()
          ..color = _chart(3 + k % 3)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5,
      );
    }

    if (layers.wicks) {
      final fill = Paint()
        ..color = c.warning
        ..style = PaintingStyle.fill;
      for (var i = 0; i < bars.length; i++) {
        if (!shootingStar(bars[i])) continue;
        final x = i * w + w / 2;
        final top = math.max(2.0, y(bars[i].high) - 7);
        final path = Path()
          ..moveTo(x, top)
          ..lineTo(x - 4, top - 7)
          ..lineTo(x + 4, top - 7)
          ..close();
        canvas.drawPath(path, fill);
      }
    }

    if (layers.gaps && bars.length >= 2) {
      final deltas = <int>[
        for (var i = 1; i < bars.length; i++) bars[i].ts - bars[i - 1].ts,
      ];
      deltas.sort();
      final step = deltas[deltas.length ~/ 2];
      if (step > 0) {
        final mark = Paint()
          ..color = c.warning.withValues(alpha: 0.85)
          ..strokeWidth = 1;
        for (var i = 1; i < bars.length; i++) {
          if (bars[i].ts - bars[i - 1].ts <= step * 3 ~/ 2) continue;
          final x = (i - 1) * w + w;
          for (var gy = 0.0; gy < priceH; gy += 5) {
            canvas.drawLine(
              Offset(x, gy),
              Offset(x, math.min(gy + 3, priceH)),
              mark,
            );
          }
        }
      }
    }

    if (layers.zeroVolume && !volOn) {
      for (var i = 0; i < bars.length; i++) {
        if (bars[i].volume != 0) continue;
        canvas.drawCircle(
          Offset(i * w + w / 2, priceH - 3),
          3,
          warn
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1.5,
        );
      }
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
      old.bars != bars ||
      old.history != history ||
      old.visibleStart != visibleStart ||
      old.c != c ||
      old.hover != hover ||
      old.layers != layers ||
      old.mas != mas ||
      old.lineMetric != lineMetric;
}

/// The TUI's Chart layers sheet (`L` / View ▸ Chart layers…).
Future<void> showChartLayersSheet(
  BuildContext context,
  WidgetRef ref, {
  int? tileId,
}) => showModalBottomSheet(
  context: context,
  showDragHandle: true,
  builder: (ctx) => Consumer(
    builder: (ctx, ref, _) {
      final ws = ref.watch(workspaceProvider);
      final layers =
          (tileId != null ? ws.tile(tileId)?.layers : null) ??
          ws.focused.layers;
      final m = ctx.muninn;
      void setLayers(ChartLayers next) {
        ref.read(chartLayersProvider.notifier).state = next;
        final cur = ref.read(workspaceProvider);
        final id = tileId ?? cur.focus;
        setWorkspace(ref, cur.updateTile(id, (t) => t.copyWith(layers: next)));
      }

      Widget tile(String title, bool value, ChartLayers Function(bool) next) =>
          SwitchListTile(
            minTileHeight: 56,
            title: Text(title),
            value: value,
            activeThumbColor: m.accent,
            onChanged: (v) => setLayers(next(v)),
          );
      return SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
              child: Text(
                'Chart layers',
                style: TextStyle(
                  color: m.header,
                  fontWeight: FontWeight.w800,
                  fontSize: 16,
                ),
              ),
            ),
            tile('Candles', layers.candles, (v) => layers.copyWith(candles: v)),
            tile('OHLC bars', layers.ohlc, (v) => layers.copyWith(ohlc: v)),
            tile('Line', layers.line, (v) => layers.copyWith(line: v)),
            tile(
              'Area under the line',
              layers.area,
              (v) => layers.copyWith(area: v),
            ),
            tile(
              'High/low envelope',
              layers.envelope,
              (v) => layers.copyWith(envelope: v),
            ),
            tile('Volume', layers.volume, (v) => layers.copyWith(volume: v)),
            tile('Moving averages', layers.ma, (v) => layers.copyWith(ma: v)),
            tile(
              'Wick markers',
              layers.wicks,
              (v) => layers.copyWith(wicks: v),
            ),
            tile('Gap markers', layers.gaps, (v) => layers.copyWith(gaps: v)),
            tile(
              'Zero-volume markers',
              layers.zeroVolume,
              (v) => layers.copyWith(zeroVolume: v),
            ),
            tile('Grid', layers.grid, (v) => layers.copyWith(grid: v)),
            tile(
              'Crosshair + value readout',
              layers.crosshair,
              (v) => layers.copyWith(crosshair: v),
            ),
            tile(
              'Price axis',
              layers.priceAxis,
              (v) => layers.copyWith(priceAxis: v),
            ),
            tile(
              'Time axis',
              layers.timeAxis,
              (v) => layers.copyWith(timeAxis: v),
            ),
            ListTile(
              minTileHeight: 56,
              title: const Text('Moving averages…'),
              subtitle: Text(
                ref.watch(chartMasProvider).map((m) => m.label).join(', '),
              ),
              onTap: () => showMovingAveragesDialog(ctx, ref),
            ),
            ListTile(
              minTileHeight: 56,
              title: const Text('Line metric'),
              subtitle: Text(ref.watch(chartLineMetricProvider).label),
              onTap: () => ref.read(chartLineMetricProvider.notifier).state =
                  ref.read(chartLineMetricProvider).next,
            ),
          ],
        ),
      );
    },
  ),
);

Future<void> showMovingAveragesDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  final saved = await showDialog<List<ChartMa>>(
    context: context,
    builder: (ctx) => const _MovingAveragesDialog(),
  );
  if (saved != null) ref.read(chartMasProvider.notifier).state = saved;
}

class _MovingAveragesDialog extends StatefulWidget {
  const _MovingAveragesDialog();

  @override
  State<_MovingAveragesDialog> createState() => _MovingAveragesDialogState();
}

class _MovingAveragesDialogState extends State<_MovingAveragesDialog> {
  late final TextEditingController text;
  String? error;

  @override
  void initState() {
    super.initState();
    text = TextEditingController();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (text.text.isEmpty) {
      final mas = ProviderScope.containerOf(
        context,
        listen: false,
      ).read(chartMasProvider);
      text.text = mas.map((m) => m.label).join(', ');
    }
  }

  @override
  void dispose() {
    text.dispose();
    super.dispose();
  }

  void _save() {
    try {
      Navigator.pop(context, parseChartMas(text.text));
    } on FormatException catch (e) {
      setState(() => error = e.message);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Row(
        children: [
          Expanded(
            child: Text(
              'Moving averages',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
            ),
          ),
          IconButton(
            tooltip: 'Close',
            icon: const Text('[X]'),
            onPressed: () => Navigator.pop(context),
          ),
        ],
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              key: const ValueKey('chart-mas'),
              controller: text,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'e.g. sma20, ema50'),
              onSubmitted: (_) => _save(),
            ),
            if (error != null) ...[
              const SizedBox(height: 12),
              Text(error!, style: TextStyle(color: m.error)),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('chart-mas-save'),
          onPressed: _save,
          child: const Text('Save'),
        ),
      ],
    );
  }
}

/// The list form of the same bars (View ▸ Change view).
class BarTable extends StatelessWidget {
  const BarTable({
    super.key,
    required this.bars,
    this.selectedTs,
    this.onSelect,
  });
  final List<BarPoint> bars;
  final int? selectedTs;
  final ValueChanged<int>? onSelect;

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    String n(double v) => v.toStringAsPrecision(6);
    final selected = selectedTs == null
        ? -1
        : nearestBarIndex(bars, selectedTs!);
    return ListView.builder(
      itemCount: bars.length,
      itemExtent: 32,
      itemBuilder: (_, i) {
        final idx = bars.length - 1 - i;
        final b = bars[idx];
        final sel = idx == selected;
        return Material(
          color: sel ? m.selectionBg : Colors.transparent,
          child: InkWell(
            onTap: onSelect == null ? null : () => onSelect!(b.ts),
            child: Row(
              children: [
                SizedBox(
                  width: 140,
                  child: Text(fmtUtc(b.ts), style: TextStyle(color: m.muted)),
                ),
                for (final v in [b.open, b.high, b.low])
                  Expanded(child: Text(n(v))),
                Expanded(
                  child: Text(
                    n(b.close),
                    style: TextStyle(color: b.close >= b.open ? m.up : m.down),
                  ),
                ),
                Expanded(
                  child: Text(n(b.volume), style: TextStyle(color: m.muted)),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
