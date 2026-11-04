import 'package:jaspr/server.dart';
import 'package:jaspr/dom.dart';

import 'app.dart';
import 'main.server.options.dart';

void main() {
  Jaspr.initializeApp(options: defaultServerOptions);
  runApp(
    Document(
      title: 'Muninn',
      lang: 'en',
      meta: {
        'description':
            'Muninn: a local, encrypted workbench for market data, with a terminal UI and a CLI.',
        'color-scheme': 'dark light',
      },
      head: [
        link(rel: 'stylesheet', href: 'styles.css'),
        link(rel: 'icon', href: 'favicon.svg', type: 'image/svg+xml'),
        link(
          rel: 'icon',
          href: 'favicon-32.png',
          type: 'image/png',
          attributes: {'sizes': '32x32'},
        ),
        link(
          rel: 'icon',
          href: 'favicon-16.png',
          type: 'image/png',
          attributes: {'sizes': '16x16'},
        ),
        link(rel: 'apple-touch-icon', href: 'apple-touch-icon.png'),
      ],
      body: const App(),
    ),
  );
}
