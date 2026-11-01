import 'package:jaspr/server.dart';
import 'package:jaspr/dom.dart';

import 'app.dart';
import 'main.server.options.dart';

void main() {
  Jaspr.initializeApp(options: defaultServerOptions);
  runApp(Document(
    title: 'Muninn',
    lang: 'en',
    meta: {
      'description': 'Muninn: a local, encrypted workbench for market data, with a terminal UI and a CLI.',
      'color-scheme': 'dark light',
    },
    head: [link(rel: 'stylesheet', href: 'styles.css')],
    body: const App(),
  ));
}
