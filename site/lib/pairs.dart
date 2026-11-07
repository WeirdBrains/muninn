// Matched screenshots: the same view, data and theme (Tokyo Night, dark) in the Flutter
// app and the terminal UI, captured from the running apps on real data.
// (title, (app file, app alt), (tui file, tui alt))
const pairs = [
  (
    'Data workspace: a list tile beside a chart of the same series',
    (
      'screenshots/pairs/app-data.png',
      'Flutter app: ETH/USDT 5m bars as a list tile beside a price and volume chart tile, the series list on the left',
    ),
    (
      'screenshots/pairs/tui-data.png',
      'Terminal UI: ETH/USDT 5m bars as a list tile beside a candle and volume chart tile, the series sidebar on the right',
    ),
  ),
  (
    'Model designer: the built-in bars model',
    (
      'screenshots/pairs/app-models.png',
      'Flutter app: the Models pane listing the built-in bars model, its 12 fields and record count',
    ),
    (
      'screenshots/pairs/tui-models.png',
      'Terminal UI: the Model designer listing the built-in bars model, its fields and record count',
    ),
  ),
];
