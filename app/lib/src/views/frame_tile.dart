/// A code-result or pipeline-view workspace tile (`code:SPACE`, `view:PIPELINE/NODE`).
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../workspace.dart';

/// Open a workspace tile for a code space's latest result (TUI `T` / `code:SPACE`).
void openCodeResultTile(WidgetRef ref, CodeSpaceInfo c) {
  ref.read(sectionProvider.notifier).state = Section.data;
  final area = ref.read(workspaceAreaProvider);
  setWorkspace(ref, ref.read(workspaceProvider).openCodeResult(c.name, area));
}

class FrameResultPane extends ConsumerWidget {
  const FrameResultPane({super.key, required this.frameKey});
  final String frameKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final hint = frameKey.startsWith('code:')
        ? 'no result yet: run the code space, or pipe data into it'
        : 'waiting for data: run the pipeline';
    return FutureBuilder<PipeFrameInfo?>(
      key: ValueKey('frame-result-$frameKey'),
      future: ref.read(apiProvider).latestPipeFrame(frameKey),
      builder: (ctx, snap) {
        if (snap.hasError) {
          return Padding(
            padding: const EdgeInsets.all(12),
            child: Text('${snap.error}', style: TextStyle(color: m.error)),
          );
        }
        if (!snap.hasData && snap.connectionState != ConnectionState.done) {
          return const Center(child: CircularProgressIndicator());
        }
        final fr = snap.data;
        if (fr == null || fr.columns.isEmpty || fr.rowCount == 0) {
          return Padding(
            padding: const EdgeInsets.all(12),
            child: Text(hint, style: TextStyle(color: m.muted)),
          );
        }
        final cols = fr.columns;
        return SingleChildScrollView(
          padding: const EdgeInsets.all(8),
          child: SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: DataTable(
              headingRowHeight: 32,
              dataRowMinHeight: 28,
              dataRowMaxHeight: 36,
              columns: [
                for (final c in cols) DataColumn(label: Text(c)),
              ],
              rows: [
                for (var i = 0; i < fr.rowCount; i++)
                  DataRow(
                    cells: [
                      for (var j = 0; j < cols.length; j++)
                        DataCell(
                          Text(
                            _cell(fr, cols.length, i, j),
                            key: j == 0
                                ? ValueKey('frame-row-$i')
                                : null,
                          ),
                        ),
                    ],
                  ),
              ],
            ),
          ),
        );
      },
    );
  }

  String _cell(PipeFrameInfo fr, int width, int row, int col) {
    final i = row * width + col;
    if (i < 0 || i >= fr.cells.length) return '';
    return fr.cells[i];
  }
}
