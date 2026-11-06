/// Orgs leftovers: templates, domains, certs/ACME, browse, fetch/give/mirror,
/// endpoints, org projects, code serve and member sign-in.
/// Themes via [context.muninn]. Certificate PEM and private keys stay in Rust
/// (Dart only passes file paths and shows public status).
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import 'orgs_dialogs.dart';

void _snack(BuildContext context, String msg, {bool error = false}) {
  if (!context.mounted) {
    return;
  }
  final m = context.muninn;
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(content: Text(msg), backgroundColor: error ? m.error : m.success),
  );
}

void _laterDispose(List<ChangeNotifier> xs) {
  // Dialog routes keep listening through the pop animation; drop them after.
  Future<void>.delayed(const Duration(milliseconds: 400), () {
    for (final x in xs) {
      x.dispose();
    }
  });
}

Future<void> _after(
  BuildContext context,
  WidgetRef ref,
  Future<String> work,
) async {
  try {
    final msg = await work;
    refreshOrgs(ref);
    if (!context.mounted) {
      return;
    }
    _snack(context, msg);
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> _showText(BuildContext context, String title, String body) async {
  final m = context.muninn;
  await showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      backgroundColor: m.bg,
      title: Text(title, style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 480,
        child: SingleChildScrollView(
          child: SelectableText(body, key: const ValueKey('org-rest-result')),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () async {
            await Clipboard.setData(ClipboardData(text: body));
            if (ctx.mounted) {
              Navigator.pop(ctx);
            }
          },
          child: const Text('Copy'),
        ),
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Close'),
        ),
      ],
    ),
  );
}

Widget _btn(String key, String label, VoidCallback onPressed) => Padding(
  padding: const EdgeInsets.only(right: 8, bottom: 8),
  child: OutlinedButton(
    key: ValueKey<String>(key),
    onPressed: onPressed,
    child: Text(label),
  ),
);

Future<String?> _pickPath() async {
  final r = await FilePicker.platform.pickFiles();
  return r?.files.single.path;
}

/// Templates, domains, certificates, endpoints, projects and served code.
class OrgMineRest extends ConsumerWidget {
  const OrgMineRest(this.org, {super.key});
  final OrgInfo org;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final api = ref.read(apiProvider);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const SizedBox(height: 8),
        Text(
          'Templates',
          style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
        ),
        if (org.templates.isEmpty)
          Text('No templates yet', style: TextStyle(color: m.muted)),
        for (final name in org.templates)
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                '  $name${org.template == name ? ' (in use)' : ''}',
                style: TextStyle(color: m.fg),
              ),
              Wrap(
                children: [
                  _btn('org-tpl-edit-$name', 'Edit', () {
                    showOrgTemplateEditDialog(context, ref, org, name);
                  }),
                  _btn('org-tpl-use-$name', 'Use', () async {
                    await _after(
                      context,
                      ref,
                      api
                          .orgEdit(org.slug, template: name)
                          .then((o) => 'template ${o.template}'),
                    );
                  }),
                  _btn('org-tpl-asset-$name', 'Add file', () async {
                    final path = await _pickPath();
                    if (path == null || !context.mounted) {
                      return;
                    }
                    await _after(
                      context,
                      ref,
                      api.orgTemplateAddAsset(org.slug, name, path),
                    );
                  }),
                  _btn('org-tpl-build-$name', 'Build…', () {
                    showOrgTemplateBuildDialog(context, ref, org, name);
                  }),
                  _btn('org-tpl-preview-$name', 'Preview', () async {
                    try {
                      final url = await api.orgTemplatePreview(org.slug, name);
                      if (context.mounted) {
                        await _showText(context, 'Preview $name', url);
                      }
                    } catch (e) {
                      if (context.mounted) {
                        _snack(context, '$e', error: true);
                      }
                    }
                  }),
                  _btn('org-tpl-remove-$name', 'Remove', () async {
                    if (!await confirmOrg(context, 'Remove template $name?')) {
                      return;
                    }
                    if (!context.mounted) {
                      return;
                    }
                    await _after(
                      context,
                      ref,
                      api.orgTemplateRemove(org.slug, name),
                    );
                  }),
                ],
              ),
            ],
          ),
        _btn('org-tpl-new-${org.slug}', 'New template', () {
          showOrgNewTemplateDialog(context, ref, org);
        }),
        const SizedBox(height: 8),
        Text(
          'Domains',
          style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
        ),
        _DomainList(org: org),
        _btn('org-domain-add-${org.slug}', 'Add domain', () {
          showOrgAddDomainDialog(context, ref, org);
        }),
        const SizedBox(height: 8),
        Text(
          'Certificate',
          style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
        ),
        _OrgCertLine(org: org),
        Wrap(
          children: [
            _btn('org-cert-set-${org.slug}', 'Set yours…', () {
              showOrgCertSetDialog(context, ref, org);
            }),
            _btn('org-cert-clear-${org.slug}', 'Clear yours', () async {
              if (!await confirmOrg(
                context,
                'Stop using your certificate for ${org.name}?',
              )) {
                return;
              }
              if (!context.mounted) {
                return;
              }
              await _after(context, ref, api.orgCertClear(org.slug));
            }),
            _btn('org-cert-acme-${org.slug}', 'Let\'s Encrypt…', () {
              showOrgAcmeDialog(context, ref, org: org);
            }),
            _btn('org-cert-renew-${org.slug}', 'Renew', () async {
              await _after(context, ref, api.orgCertRenew(org: org.slug));
            }),
          ],
        ),
        const SizedBox(height: 8),
        Text(
          'Endpoints',
          style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
        ),
        if (org.endpoints.isEmpty)
          Text('No endpoints yet', style: TextStyle(color: m.muted)),
        for (final e in org.endpoints)
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                '  ${e.name}  ${e.kind}  needs ${e.perm}${e.anyone ? ' (or anyone)' : ''}',
                style: TextStyle(color: m.fg),
              ),
              Wrap(
                children: [
                  _btn('org-ep-call-${e.name}', 'Call', () {
                    showOrgCallDialog(context, ref, org.slug, e.name);
                  }),
                  _btn('org-ep-remove-${e.name}', 'Remove', () async {
                    if (!await confirmOrg(
                      context,
                      'Remove endpoint ${e.name}?',
                    )) {
                      return;
                    }
                    if (!context.mounted) {
                      return;
                    }
                    await _after(
                      context,
                      ref,
                      api.orgEndpointRemove(org.slug, e.name),
                    );
                  }),
                ],
              ),
            ],
          ),
        _btn('org-ep-add-${org.slug}', 'Add endpoint', () {
          showOrgAddEndpointDialog(context, ref, org);
        }),
        const SizedBox(height: 8),
        Text(
          'Projects',
          style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
        ),
        if (org.projects.isEmpty)
          Text('No org projects yet', style: TextStyle(color: m.muted)),
        for (final n in org.projects)
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('  $n', style: TextStyle(color: m.fg)),
              Wrap(
                children: [
                  _btn('org-proj-member-$n', 'Add member…', () {
                    showOrgProjectMemberDialog(context, ref, org, n);
                  }),
                  _btn('org-proj-remove-$n', 'Detach', () async {
                    if (!await confirmOrg(
                      context,
                      '$n is no longer a project of ${org.name}? (it stays here)',
                    )) {
                      return;
                    }
                    if (!context.mounted) {
                      return;
                    }
                    await _after(
                      context,
                      ref,
                      api.orgProjectRemove(org.slug, n),
                    );
                  }),
                ],
              ),
            ],
          ),
        _btn('org-proj-add-${org.slug}', 'Attach project', () {
          showOrgAttachProjectDialog(context, ref, org);
        }),
        const SizedBox(height: 8),
        Text(
          'Code this node serves',
          style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
        ),
        _ServedCode(org: org),
      ],
    );
  }
}

class _DomainList extends ConsumerWidget {
  const _DomainList({required this.org});
  final OrgInfo org;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return FutureBuilder<List<OrgDomainInfo>>(
      future: ref.read(apiProvider).orgDomains(org: org.slug),
      builder: (context, snap) {
        final rows = snap.data ?? const <OrgDomainInfo>[];
        if (snap.hasError) {
          return Text('${snap.error}', style: TextStyle(color: m.error));
        }
        if (rows.isEmpty) {
          return Text('No domains yet', style: TextStyle(color: m.muted));
        }
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final d in rows)
              Row(
                children: [
                  Expanded(
                    child: Text('  ${d.domain}', style: TextStyle(color: m.fg)),
                  ),
                  _btn('org-domain-rm-${d.domain}', 'Remove', () async {
                    await _after(
                      context,
                      ref,
                      ref.read(apiProvider).orgDomainRemove(org.slug, d.domain),
                    );
                  }),
                ],
              ),
          ],
        );
      },
    );
  }
}

class _OrgCertLine extends ConsumerWidget {
  const _OrgCertLine({required this.org});
  final OrgInfo org;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return FutureBuilder<OrgCertOverview>(
      future: ref.read(apiProvider).orgCertStatus(org: org.slug),
      builder: (context, snap) {
        if (snap.hasError) {
          return Text('${snap.error}', style: TextStyle(color: m.error));
        }
        final c = snap.data?.orgs.isNotEmpty == true
            ? snap.data!.orgs.first
            : null;
        if (c == null) {
          return Text(
            'self-signed when the host starts (no PEM shown)',
            style: TextStyle(color: m.muted),
          );
        }
        final names = c.names.isEmpty
            ? c.domains.join(', ')
            : c.names.join(', ');
        final retry = c.retryAttempts > 0
            ? ' · ACME failed ${c.retryAttempts}× ${c.retryError}'
            : '';
        return Text(
          '${c.source}${names.isEmpty ? '' : ' for $names'}${c.notAfter.isEmpty ? '' : ' until ${c.notAfter}'}${c.acmeEnabled ? ' · ACME on' : ''}$retry',
          style: TextStyle(color: m.fg),
        );
      },
    );
  }
}

class _ServedCode extends ConsumerWidget {
  const _ServedCode({required this.org});
  final OrgInfo org;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return FutureBuilder<List<OrgCodeServedInfo>>(
      future: ref.read(apiProvider).orgCodeList(org.slug),
      builder: (context, snap) {
        final rows = snap.data ?? const <OrgCodeServedInfo>[];
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (rows.isEmpty)
              Text(
                'This node serves no code for ${org.name}',
                style: TextStyle(color: m.muted),
              ),
            for (final c in rows)
              Row(
                children: [
                  Expanded(
                    child: Text(
                      '  ${c.name}  ${c.lang}',
                      style: TextStyle(color: m.fg),
                    ),
                  ),
                  _btn('org-code-stop-${c.name}', 'Stop', () async {
                    await _after(
                      context,
                      ref,
                      ref.read(apiProvider).orgCodeStop(org.slug, c.name),
                    );
                  }),
                ],
              ),
            _btn('org-code-serve-${org.slug}', 'Serve a space…', () {
              showOrgCodeServeDialog(context, ref, org);
            }),
          ],
        );
      },
    );
  }
}

/// Fetch, give, mirror, sync-code and sign-in for a joined org.
class OrgJoinedRest extends ConsumerWidget {
  const OrgJoinedRest(this.j, {super.key});
  final OrgJoined j;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Wrap(
      children: [
        _btn('org-fetch-${j.slug}', 'Fetch…', () {
          showOrgFetchDialog(context, ref, j);
        }),
        _btn('org-give-${j.slug}', 'Give…', () {
          showOrgGiveDialog(context, ref, j);
        }),
        _btn('org-mirror-${j.slug}', 'Mirror', () async {
          await _after(context, ref, ref.read(apiProvider).orgMirror(j.slug));
        }),
        _btn('org-sync-code-${j.slug}', 'Sync code', () async {
          try {
            final rows = await ref.read(apiProvider).orgSyncCode(j.slug);
            refreshOrgs(ref);
            if (!context.mounted) {
              return;
            }
            if (rows.isEmpty) {
              _snack(context, 'no code spaces of ${j.name} here');
            } else {
              await _showText(
                context,
                'Code of ${j.name}',
                rows.map((x) => '${x.state}\t${x.name}').join('\n'),
              );
            }
          } catch (e) {
            if (context.mounted) {
              _snack(context, '$e', error: true);
            }
          }
        }),
        _btn('org-signin-${j.slug}', 'Sign in…', () {
          showOrgSignInDialog(context, ref, j);
        }),
      ],
    );
  }
}

Future<void> showOrgBrowseDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.orgs;
  final m = context.muninn;
  final target = TextEditingController();
  final org = TextEditingController();
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('Browse organizations', style: TextStyle(color: m.header)),
        content: SizedBox(
          width: 420,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                key: const ValueKey('org-browse-target'),
                controller: target,
                decoration: const InputDecoration(
                  labelText: 'Node (…/p2p/ID or a known peer)',
                ),
              ),
              TextField(
                key: const ValueKey('org-browse-org'),
                controller: org,
                decoration: const InputDecoration(
                  labelText: 'Organization (optional)',
                ),
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-browse-go'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Browse'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    final rows = await ref
        .read(apiProvider)
        .orgBrowse(
          target.text.trim(),
          org: org.text.trim().isEmpty ? null : org.text.trim(),
        );
    if (!context.mounted) {
      return;
    }
    if (rows.isEmpty) {
      await _showText(context, 'Browse', 'That node lists no organizations.');
    } else {
      await _showText(
        context,
        'Browse',
        rows
            .map(
              (c) =>
                  '${c.name}  ${c.slug}  ${c.visibility}  ${c.access}  ${c.members} members  ${c.description}',
            )
            .join('\n'),
      );
    }
  } catch (e) {
    if (context.mounted) {
      _snack(context, '$e', error: true);
    }
  } finally {
    _laterDispose([target, org]);
  }
}

Future<void> showOrgCertsDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.orgs;
  try {
    final st = await ref.read(apiProvider).orgCertStatus();
    if (!context.mounted) {
      return;
    }
    final n = st.node;
    final buf = StringBuffer();
    buf.writeln(
      n.source.isEmpty
          ? 'certificate: self-signed, made when the host starts, valid ${n.selfSignedDays} days'
          : 'certificate: ${n.source} for ${n.names.join(', ')} until ${n.notAfter} (${n.issuer})',
    );
    if (n.acmeEnabled) {
      buf.writeln(
        'ACME: ${n.acmeDirectory} for ${n.acmeDomain} (${n.acmeEmail}), profile ${n.acmeProfile.isEmpty ? 'default' : n.acmeProfile}, ${n.acmeChallenge} on port ${n.acmeHttpPort}',
      );
    } else {
      buf.writeln('ACME: off');
    }
    for (final c in st.orgs) {
      buf.writeln(
        '${c.org} (${c.slug}): ${c.source}${c.names.isEmpty ? '' : ' for ${c.names.join(', ')}'}${c.notAfter.isEmpty ? '' : ' until ${c.notAfter}'}${c.acmeEnabled ? ' · ACME on' : ''}',
      );
    }
    await _showText(context, 'Certificates', buf.toString().trim());
  } catch (e) {
    if (context.mounted) {
      _snack(context, '$e', error: true);
    }
  }
}

Future<void> showOrgNewTemplateDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  final m = context.muninn;
  final name = TextEditingController();
  final useIt = ValueNotifier(true);
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('New template', style: TextStyle(color: m.header)),
        content: SizedBox(
          width: 420,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                key: const ValueKey('org-tpl-name'),
                controller: name,
                decoration: const InputDecoration(labelText: 'Name'),
              ),
              ValueListenableBuilder(
                valueListenable: useIt,
                builder: (_, v, _) => CheckboxListTile(
                  key: const ValueKey('org-tpl-use'),
                  title: const Text('Use it for the site'),
                  value: v,
                  onChanged: (x) => useIt.value = x ?? false,
                ),
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-tpl-save'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    await _after(
      context,
      ref,
      ref
          .read(apiProvider)
          .orgTemplateSave(org.slug, name.text.trim(), useIt: useIt.value)
          .then((t) => 'template ${t.name}${t.inUse ? ', in use' : ''}'),
    );
  } finally {
    _laterDispose([name, useIt]);
  }
}

Future<void> showOrgTemplateEditDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
  String template,
) async {
  final m = context.muninn;
  OrgTemplateInfo? shown;
  try {
    shown = await ref.read(apiProvider).orgTemplateShow(org.slug, template);
  } catch (e) {
    if (context.mounted) {
      _snack(context, '$e', error: true);
    }
    return;
  }
  if (!context.mounted) {
    return;
  }
  final html = TextEditingController(text: shown.html);
  final css = TextEditingController(text: shown.css);
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('Edit ${shown!.name}', style: TextStyle(color: m.header)),
        content: SizedBox(
          width: 520,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                TextField(
                  key: const ValueKey('org-tpl-html'),
                  controller: html,
                  maxLines: 10,
                  decoration: const InputDecoration(labelText: 'HTML'),
                ),
                TextField(
                  key: const ValueKey('org-tpl-css'),
                  controller: css,
                  maxLines: 6,
                  decoration: const InputDecoration(labelText: 'CSS'),
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-tpl-edit-save'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    await _after(
      context,
      ref,
      ref
          .read(apiProvider)
          .orgTemplateSave(org.slug, shown.name, html: html.text, css: css.text)
          .then((t) => 'template ${t.name} saved'),
    );
  } finally {
    _laterDispose([html, css]);
  }
}

Future<void> showOrgTemplateBuildDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
  String template,
) async {
  final m = context.muninn;
  final cmd = TextEditingController();
  final src = TextEditingController();
  final out = TextEditingController();
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('Build $template', style: TextStyle(color: m.header)),
        content: SizedBox(
          width: 420,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                key: const ValueKey('org-tpl-build-cmd'),
                controller: cmd,
                decoration: const InputDecoration(
                  labelText: 'Command (empty keeps the saved one)',
                ),
              ),
              TextField(
                controller: src,
                decoration: const InputDecoration(labelText: 'Source folder'),
              ),
              TextField(
                controller: out,
                decoration: const InputDecoration(
                  labelText: 'Output folder (default build)',
                ),
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-tpl-build-run'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Run'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    await _after(
      context,
      ref,
      ref
          .read(apiProvider)
          .orgTemplateBuild(
            org.slug,
            template,
            cmd: cmd.text.trim().isEmpty ? null : cmd.text.trim(),
            src: src.text.trim().isEmpty ? null : src.text.trim(),
            out: out.text.trim().isEmpty ? null : out.text.trim(),
            approve: true,
          ),
    );
  } finally {
    _laterDispose([cmd, src, out]);
  }
}

Future<void> showOrgAddDomainDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  final m = context.muninn;
  final domain = TextEditingController();
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text(
          'Domain for ${org.name}',
          style: TextStyle(color: m.header),
        ),
        content: TextField(
          key: const ValueKey('org-domain'),
          controller: domain,
          decoration: const InputDecoration(labelText: 'lab.example.org'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-domain-save'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Add'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    await _after(
      context,
      ref,
      ref.read(apiProvider).orgDomainAdd(org.slug, domain.text.trim()),
    );
  } finally {
    _laterDispose([domain]);
  }
}

Future<void> showOrgCertSetDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  final m = context.muninn;
  final cert = TextEditingController();
  final key = TextEditingController();
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text(
          'Your certificate for ${org.name}',
          style: TextStyle(color: m.header),
        ),
        content: SizedBox(
          width: 420,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                'Pick the PEM files. Rust reads them; the key never comes into the app.',
                style: TextStyle(color: m.muted),
              ),
              TextField(
                key: const ValueKey('org-cert-path'),
                controller: cert,
                decoration: const InputDecoration(labelText: 'Certificate PEM'),
              ),
              TextButton(
                onPressed: () async {
                  final p = await _pickPath();
                  if (p != null) {
                    cert.text = p;
                  }
                },
                child: const Text('Browse certificate…'),
              ),
              TextField(
                key: const ValueKey('org-cert-key-path'),
                controller: key,
                decoration: const InputDecoration(labelText: 'Private key PEM'),
              ),
              TextButton(
                onPressed: () async {
                  final p = await _pickPath();
                  if (p != null) {
                    key.text = p;
                  }
                },
                child: const Text('Browse key…'),
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-cert-set-save'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Set'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    if (cert.text.trim().isEmpty || key.text.trim().isEmpty) {
      _snack(context, 'pick the certificate and key files', error: true);
      return;
    }
    await _after(
      context,
      ref,
      ref
          .read(apiProvider)
          .orgCertSet(org.slug, cert.text.trim(), key.text.trim()),
    );
  } finally {
    _laterDispose([cert, key]);
  }
}

Future<void> showOrgAcmeDialog(
  BuildContext context,
  WidgetRef ref, {
  OrgInfo? org,
}) async {
  final m = context.muninn;
  final email = TextEditingController();
  final domain = TextEditingController();
  try {
    final ok = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text(
          org == null
              ? 'Let\'s Encrypt (this node)'
              : 'Let\'s Encrypt for ${org.name}',
          style: TextStyle(color: m.header),
        ),
        content: SizedBox(
          width: 420,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                key: const ValueKey('org-acme-email'),
                controller: email,
                decoration: const InputDecoration(labelText: 'Contact email'),
              ),
              if (org == null)
                TextField(
                  key: const ValueKey('org-acme-domain'),
                  controller: domain,
                  decoration: const InputDecoration(labelText: 'Domain'),
                ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-acme-off'),
            onPressed: () => Navigator.pop(ctx, 'off'),
            child: const Text('Off'),
          ),
          TextButton(
            key: const ValueKey('org-acme-on'),
            onPressed: () => Navigator.pop(ctx, 'on'),
            child: const Text('On'),
          ),
        ],
      ),
    );
    if (!context.mounted || ok == null) {
      return;
    }
    await _after(
      context,
      ref,
      ref
          .read(apiProvider)
          .orgCertAcme(
            org: org?.slug,
            email: email.text.trim().isEmpty ? null : email.text.trim(),
            domain: org == null && domain.text.trim().isNotEmpty
                ? domain.text.trim()
                : null,
            off: ok == 'off',
          ),
    );
  } finally {
    _laterDispose([email, domain]);
  }
}

Future<void> showOrgAddEndpointDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  final m = context.muninn;
  final name = TextEditingController();
  final target = TextEditingController();
  String kind = 'records';
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setSt) => AlertDialog(
          backgroundColor: m.bg,
          title: Text('Add endpoint', style: TextStyle(color: m.header)),
          content: SizedBox(
            width: 420,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                TextField(
                  key: const ValueKey('org-ep-name'),
                  controller: name,
                  decoration: const InputDecoration(labelText: 'Name'),
                ),
                DropdownButtonFormField<String>(
                  key: const ValueKey('org-ep-kind'),
                  initialValue: kind,
                  items: [
                    for (final k in ['records', 'rule', 'code'])
                      DropdownMenuItem(value: k, child: Text(k)),
                  ],
                  onChanged: (v) {
                    if (v != null) {
                      setSt(() => kind = v);
                    }
                  },
                  decoration: const InputDecoration(labelText: 'Kind'),
                ),
                TextField(
                  key: const ValueKey('org-ep-target'),
                  controller: target,
                  decoration: const InputDecoration(
                    labelText: 'Model, rule or code space',
                  ),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('Cancel'),
            ),
            TextButton(
              key: const ValueKey('org-ep-save'),
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('Add'),
            ),
          ],
        ),
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    await _after(
      context,
      ref,
      ref
          .read(apiProvider)
          .orgEndpointAdd(org.slug, name.text.trim(), kind, target.text.trim()),
    );
  } finally {
    _laterDispose([name, target]);
  }
}

Future<void> showOrgCallDialog(
  BuildContext context,
  WidgetRef ref,
  String org,
  String endpoint,
) async {
  final m = context.muninn;
  final input = TextEditingController(text: '{}');
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('Call $endpoint', style: TextStyle(color: m.header)),
        content: TextField(
          key: const ValueKey('org-call-input'),
          controller: input,
          maxLines: 6,
          decoration: const InputDecoration(labelText: 'JSON input'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-call-go'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Call'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    final out = await ref
        .read(apiProvider)
        .orgCall(org, endpoint, input: input.text);
    if (context.mounted) {
      await _showText(context, 'Call $endpoint', out);
    }
  } catch (e) {
    if (context.mounted) {
      _snack(context, '$e', error: true);
    }
  } finally {
    _laterDispose([input]);
  }
}

Future<void> showOrgAttachProjectDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  final m = context.muninn;
  final name = TextEditingController();
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('Attach a project', style: TextStyle(color: m.header)),
        content: TextField(
          key: const ValueKey('org-proj-name'),
          controller: name,
          decoration: const InputDecoration(labelText: 'Project'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-proj-save'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Attach'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    await _after(
      context,
      ref,
      ref.read(apiProvider).orgProjectAdd(org.slug, name.text.trim()),
    );
  } finally {
    _laterDispose([name]);
  }
}

Future<void> showOrgProjectMemberDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
  String project,
) async {
  final m = context.muninn;
  final member = TextEditingController();
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('Add to $project', style: TextStyle(color: m.header)),
        content: TextField(
          key: const ValueKey('org-proj-member'),
          controller: member,
          decoration: const InputDecoration(labelText: 'Member'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Add'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    await _after(
      context,
      ref,
      ref
          .read(apiProvider)
          .orgProjectMember(org.slug, project, member.text.trim()),
    );
  } finally {
    _laterDispose([member]);
  }
}

Future<void> showOrgCodeServeDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  final m = context.muninn;
  final space = TextEditingController();
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('Serve a code space', style: TextStyle(color: m.header)),
        content: TextField(
          key: const ValueKey('org-code-space'),
          controller: space,
          decoration: const InputDecoration(labelText: 'Code space'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-code-serve-save'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Serve'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    await _after(
      context,
      ref,
      ref.read(apiProvider).orgCodeServe(org.slug, space.text.trim()),
    );
  } finally {
    _laterDispose([space]);
  }
}

Future<void> showOrgFetchDialog(
  BuildContext context,
  WidgetRef ref,
  OrgJoined j,
) async {
  final m = context.muninn;
  final resource = TextEditingController();
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('Fetch from ${j.name}', style: TextStyle(color: m.header)),
        content: TextField(
          key: const ValueKey('org-fetch-resource'),
          controller: resource,
          decoration: const InputDecoration(labelText: 'Resource'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-fetch-go'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Fetch'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    final r = await ref
        .read(apiProvider)
        .orgFetch(j.slug, resource.text.trim());
    if (!context.mounted) {
      return;
    }
    final body = StringBuffer('${r.name} (${r.format}, ${r.size} bytes)');
    if (r.written.isNotEmpty) {
      body.writeln('\nwrote ${r.written}');
    }
    if (r.text != null) {
      body.writeln('\n${r.text}');
    }
    await _showText(context, 'Fetch', body.toString());
  } catch (e) {
    if (context.mounted) {
      _snack(context, '$e', error: true);
    }
  } finally {
    _laterDispose([resource]);
  }
}

Future<void> showOrgGiveDialog(
  BuildContext context,
  WidgetRef ref,
  OrgJoined j,
) async {
  final m = context.muninn;
  final name = TextEditingController();
  String kind = 'journal';
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setSt) => AlertDialog(
          backgroundColor: m.bg,
          title: Text('Give to ${j.name}', style: TextStyle(color: m.header)),
          content: SizedBox(
            width: 420,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                DropdownButtonFormField<String>(
                  key: const ValueKey('org-give-kind'),
                  initialValue: kind,
                  items: [
                    for (final k in [
                      'model',
                      'search',
                      'journal',
                      'project',
                      'report',
                      'code',
                    ])
                      DropdownMenuItem(value: k, child: Text(k)),
                  ],
                  onChanged: (v) {
                    if (v != null) {
                      setSt(() => kind = v);
                    }
                  },
                  decoration: const InputDecoration(labelText: 'Kind'),
                ),
                TextField(
                  key: const ValueKey('org-give-name'),
                  controller: name,
                  decoration: const InputDecoration(labelText: 'Name'),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('Cancel'),
            ),
            TextButton(
              key: const ValueKey('org-give-save'),
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('Give'),
            ),
          ],
        ),
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    await _after(
      context,
      ref,
      ref.read(apiProvider).orgGive(j.slug, kind, name.text.trim()),
    );
  } finally {
    _laterDispose([name]);
  }
}

Future<void> showOrgSignInDialog(
  BuildContext context,
  WidgetRef ref,
  OrgJoined j,
) async {
  final m = context.muninn;
  final site = TextEditingController();
  try {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('Sign in to ${j.name}', style: TextStyle(color: m.header)),
        content: TextField(
          key: const ValueKey('org-signin-site'),
          controller: site,
          decoration: const InputDecoration(
            labelText: 'Site address (https://lab.example.org:4743)',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const ValueKey('org-signin-go'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Link'),
          ),
        ],
      ),
    );
    if (ok != true || !context.mounted) {
      return;
    }
    final url = await ref.read(apiProvider).orgSignIn(j.slug, site.text.trim());
    if (context.mounted) {
      await _showText(context, 'Sign-in link', url);
    }
  } catch (e) {
    if (context.mounted) {
      _snack(context, '$e', error: true);
    }
  } finally {
    _laterDispose([site]);
  }
}

Future<void> showOrgNodeAcmeDialog(BuildContext context, WidgetRef ref) async {
  await showOrgAcmeDialog(context, ref);
}
