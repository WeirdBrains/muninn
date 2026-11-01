// Written by scripts/screenshots.sh from the TUI screenshot test; do not edit.
// The hero shot, then every feature shot: (file, caption).
const heroShot = (
  'screenshots/pipeline-live.png',
  'cat ticks.csv | mnnn pipeline run feed -: three views as three live columns',
);
const shots = [
  (
    'screenshots/home.png',
    'Home: every series in a list, the header with the node\'s status',
  ),
  (
    'screenshots/home-chart.png',
    'A chart tile beside the list: ETH/USDT 5m candles in the Muninn theme',
  ),
  (
    'screenshots/history.png',
    'The action history bar: every step, forks kept, step back and on',
  ),
  (
    'screenshots/live-searches.png',
    'Live searches: saved questions that rerun on new data',
  ),
  (
    'screenshots/share-view-name.png',
    'Share view, mid-action: saving the view as a live search',
  ),
  (
    'screenshots/share-view-picker.png',
    'Share: pick where it goes (a project, an org, a connection)',
  ),
  ('screenshots/journals.png', 'Journals with their entries'),
  ('screenshots/journal-entries.png', 'A journal\'s entries in Markdown'),
  ('screenshots/share-picker.png', 'Sharing a journal entry: the picker open'),
  (
    'screenshots/reports.png',
    'Reports: sections of text, series and journals, exported to PDF, Docx or HTML',
  ),
  ('screenshots/report-sections.png', 'A report\'s sections'),
  (
    'screenshots/rules.png',
    'Rule sets: on actions or on a schedule (every 1h, cron), with the built-in cert watcher',
  ),
  (
    'screenshots/rule-scheduled.png',
    'A scheduled rule: its schedule, next run and then-actions',
  ),
  (
    'screenshots/rule-new.png',
    'New rule, mid-action: "on" takes action kinds or a schedule',
  ),
  (
    'screenshots/models.png',
    'Model designer: models, the built-in bars model among them',
  ),
  (
    'screenshots/model-plan.png',
    'Designing a model: the migration plan before apply',
  ),
  ('screenshots/model-records.png', 'Records of the new model'),
  (
    'screenshots/model-template-import.png',
    'Model templates, mid-action: importing a design (planned first, rename or merge)',
  ),
  (
    'screenshots/code-new.png',
    'New code space, mid-action: choosing a language (math, Polars, Rust, Python)',
  ),
  ('screenshots/code-editor.png', 'Editing a code space'),
  ('screenshots/code-list.png', 'Code spaces and their scope'),
  (
    'screenshots/code-chart.png',
    'A code space\'s result as a live chart tile beside the series',
  ),
  (
    'screenshots/pipeline-live.png',
    'cat ticks.csv | mnnn pipeline run feed -: three views as three live columns',
  ),
  (
    'screenshots/pipeline-monitor.png',
    'The pipeline monitor: each node\'s status, rows, rate and what landed',
  ),
  (
    'screenshots/pipes.png',
    'Pipes: data from stdin or any search through code spaces, followed live',
  ),
  (
    'screenshots/pipeline-designer.png',
    'The pipeline designer: a graph of sources, code, views and model tables',
  ),
  (
    'screenshots/pipeline-designer-add.png',
    'Pipeline designer, mid-action: adding a code node',
  ),
  (
    'screenshots/orgs.png',
    'Organizations: visibility, access, hosting, domains (w), certificate (c), ACME (L)',
  ),
  ('screenshots/org-domain.png', 'Org TLS, mid-action: adding a domain'),
  (
    'screenshots/org-site.png',
    'The org site: each org\'s domains and certificate (yours, Let\'s Encrypt or self-signed)',
  ),
  (
    'screenshots/reviews.png',
    'Reviews: comments, suggestions, highlights and votes on shared work',
  ),
  (
    'screenshots/network.png',
    'The network: this node, relays, connections and shares, end to end encrypted',
  ),
  ('screenshots/peers.png', 'Peers: nodes seen nearby and through relays'),
  (
    'screenshots/connections.png',
    'Connections: people you trust, made through invites',
  ),
];
