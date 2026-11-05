/// A price chart in the theme's colours: candles when they fit, else a close line, with
/// volume underneath. Bars arrive already downsampled by Rust.
library;

import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../api.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';

class BarChart extends StatelessWidget {
  const BarChart({super.key, required this.bars});
  final List<BarPoint> bars;

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    if (bars.isEmpty) return Center(child: Text('No bars in this time range', style: TextStyle(color: m.muted)));
    final hi = bars.map((b) => b.high).reduce(math.max);
    final lo = bars.map((b) => b.low).reduce(math.min);
    final label = TextStyle(color: m.muted, fontSize: 11);
    return Semantics(
      label: 'Price chart, ${bars.length} bars',
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Row(children: [
          Text(hi.toStringAsPrecision(6), style: label),
          const Spacer(),
          Text('${bars.length} bars', style: label),
        ]),
        Expanded(child: CustomPaint(painter: BarChartPainter(bars, m), size: Size.infinite)),
        Row(children: [
          Text(lo.toStringAsPrecision(6), style: label),
          const Spacer(),
        ]),
        Row(children: [
          Flexible(child: Text(fmtUtc(bars.first.ts), style: label, overflow: TextOverflow.ellipsis)),
          const Spacer(),
          Flexible(child: Text('${fmtUtc(bars.last.ts)} UTC', style: label, overflow: TextOverflow.ellipsis, textAlign: TextAlign.end)),
        ]),
      ]),
    );
  }
}

class BarChartPainter extends CustomPainter {
  BarChartPainter(this.bars, this.c);
  final List<BarPoint> bars;
  final MuninnColors c;

  /// Candles need at least this many pixels each.
  static const minCandle = 3.0;

  bool candlesFit(double width) => width / bars.length >= minCandle;

  @override
  void paint(Canvas canvas, Size size) {
    if (bars.isEmpty || size.isEmpty) return;
    final priceH = size.height * 0.8;
    final volTop = priceH + 4;
    final volH = size.height - volTop;
    final hi = bars.map((b) => b.high).reduce(math.max);
    final lo = bars.map((b) => b.low).reduce(math.min);
    final span = (hi - lo) == 0 ? 1.0 : hi - lo;
    final vmax = bars.map((b) => b.volume).fold<double>(0, math.max);
    double y(double p) => priceH - (p - lo) / span * priceH;
    final w = size.width / bars.length;
    final grid = Paint()
      ..color = c.border
      ..strokeWidth = 1;
    for (var i = 0; i <= 4; i++) {
      final gy = priceH * i / 4;
      canvas.drawLine(Offset(0, gy), Offset(size.width, gy), grid);
    }
    final up = Paint()..color = c.up;
    final down = Paint()..color = c.down;
    if (candlesFit(size.width)) {
      for (var i = 0; i < bars.length; i++) {
        final b = bars[i];
        final x = i * w + w / 2;
        final p = b.close >= b.open ? up : down;
        canvas.drawLine(Offset(x, y(b.high)), Offset(x, y(b.low)), p..strokeWidth = 1);
        final top = y(math.max(b.open, b.close)), bot = y(math.min(b.open, b.close));
        canvas.drawRect(Rect.fromLTRB(x - w * 0.35, top, x + w * 0.35, math.max(bot, top + 1)), p);
      }
    } else {
      final path = Path();
      for (var i = 0; i < bars.length; i++) {
        final pt = Offset(i * w + w / 2, y(bars[i].close));
        i == 0 ? path.moveTo(pt.dx, pt.dy) : path.lineTo(pt.dx, pt.dy);
      }
      canvas.drawPath(
          path,
          Paint()
            ..color = c.chart.first
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1.5);
    }
    if (vmax > 0 && volH > 2) {
      final vp = Paint()..color = c.chart.length > 1 ? c.chart[1].withValues(alpha: 0.6) : c.accent;
      for (var i = 0; i < bars.length; i++) {
        final h = bars[i].volume / vmax * volH;
        canvas.drawRect(Rect.fromLTWH(i * w, size.height - h, math.max(w - 1, 1), h), vp);
      }
    }
  }

  @override
  bool shouldRepaint(BarChartPainter old) => old.bars != bars || old.c != c;
}

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
