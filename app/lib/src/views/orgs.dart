/// Orgs (read-only): `mnnn org list` and the fields `mnnn org show` prints.
/// Create, join, edit and hosting stay in the TUI or CLI. The store is opened
/// read-only; no keys or mirror snapshots are sent to Dart.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';
import 'unlock.dart';

final _selectedOrg = StateProvider<String?>((ref) => null);

class OrgsView extends ConsumerWidget {
  const OrgsView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return Unlocked(
      child: ref
          .watch(orgOverviewProvider)
          .when(
            loading: () => const LinearProgressIndicator(),
            error: (e, _) => Padding(
              padding: const EdgeInsets.all(16),
              child: Text('$e', style: TextStyle(color: m.error)),
            ),
            data: (o) {
              if (o == null) return const SizedBox.shrink();
              final empty =
                  o.mine.isEmpty && o.joined.isEmpty && o.mirrors.isEmpty;
              return ListView(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
                children: [
                  if (empty)
                    Text(
                      'No organizations yet: `mnnn org create NAME`, or `mnnn org browse NODE` and `mnnn org apply`.',
                      style: TextStyle(color: m.muted),
                    )
                  else ...[
                    _heading(m, 'Organizations'),
                    if (o.mine.isEmpty)
                      Text('None here yet', style: TextStyle(color: m.muted)),
                    for (final org in o.mine) _MineTile(org),
                    const SizedBox(height: 20),
                    _heading(m, 'Joined'),
                    if (o.joined.isEmpty)
                      Text(
                        'None yet: `mnnn org browse NODE` and `mnnn org apply`, or join with an invitation.',
                        style: TextStyle(color: m.muted),
                      )
                    else
                      for (final j in o.joined) _JoinedTile(j),
                    if (o.mirrors.isNotEmpty) ...[
                      const SizedBox(height: 20),
                      _heading(m, 'Mirrors'),
                      for (final x in o.mirrors)
                        ListTile(
                          contentPadding: EdgeInsets.zero,
                          leading: Icon(Icons.copy_outlined, color: m.muted),
                          title: Text(x.slug),
                          subtitle: Text(
                            x.fetchedAt == 0
                                ? 'fetched'
                                : 'fetched ${fmtUtc(x.fetchedAt)} UTC',
                            style: TextStyle(color: m.muted),
                          ),
                        ),
                    ],
                  ],
                  const SizedBox(height: 24),
                  Text(
                    'Create, edit, join, leave and host organizations with `mnnn org` or the TUI (G).',
                    style: TextStyle(color: m.muted),
                  ),
                ],
              );
            },
          ),
    );
  }
}

Widget _heading(MuninnColors m, String t) => Padding(
  padding: const EdgeInsets.only(bottom: 8),
  child: Text(
    t,
    style: TextStyle(
      color: m.header,
      fontWeight: FontWeight.w800,
      fontSize: 16,
    ),
  ),
);

class _MineTile extends ConsumerWidget {
  const _MineTile(this.org);
  final OrgInfo org;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final open = ref.watch(_selectedOrg) == org.id;
    final flags = [
      org.visibility,
      org.access,
      if (org.hosted) 'hosted',
      if (org.gated) 'gated',
    ].join(' · ');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ListTile(
          key: ValueKey('org-${org.slug}'),
          contentPadding: EdgeInsets.zero,
          leading: Icon(Icons.apartment_outlined, color: m.accent),
          title: Text(org.name),
          subtitle: Text(
            '${org.slug} · $flags · ${org.members} members',
            style: TextStyle(color: m.muted),
          ),
          trailing: Icon(open ? Icons.expand_less : Icons.expand_more),
          onTap: () =>
              ref.read(_selectedOrg.notifier).state = open ? null : org.id,
        ),
        if (open) _MineDetail(org),
      ],
    );
  }
}

class _MineDetail extends StatelessWidget {
  const _MineDetail(this.org);
  final OrgInfo org;

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return Padding(
      padding: const EdgeInsets.fromLTRB(56, 0, 0, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (org.description.isNotEmpty) ...[
            Text(org.description),
            const SizedBox(height: 8),
          ],
          Text(
            'New members: ${org.applyRole} (apply) / ${org.inviteRole} (invite)',
            style: TextStyle(color: m.muted),
          ),
          Text(
            'Site: ${org.hosted ? 'hosted' : 'not hosted'}${org.gated ? ' (gated)' : ''}; member sign-in ${org.memberAuth ? 'on' : 'off'}; template ${org.template}',
            style: TextStyle(color: m.muted),
          ),
          if (org.roles.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              'Roles',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
            ),
            for (final r in org.roles)
              Text(
                '  ${r.name}  rank ${r.rank}  ${r.perms.join(' ')}',
                style: TextStyle(color: m.fg),
              ),
          ],
          if (org.memberList.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              'Members',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
            ),
            for (final x in org.memberList)
              Text(
                '  ${x.title}  ${x.role}  ${x.status}${x.note.isEmpty ? '' : '  ${x.note}'}',
                style: TextStyle(color: m.fg),
              ),
          ],
          if (org.resources.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              'Resources',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
            ),
            for (final r in org.resources)
              Text(
                '  ${r.kind}  ${r.name}  ${r.state}  published: ${r.published}',
                style: TextStyle(color: m.fg),
              ),
          ],
          if (org.projects.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              'Projects',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
            ),
            for (final n in org.projects)
              Text('  $n', style: TextStyle(color: m.fg)),
          ],
          if (org.endpoints.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              'Endpoints',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
            ),
            for (final e in org.endpoints)
              Text(
                '  ${e.name}  ${e.kind}  needs ${e.perm}${e.anyone ? ' (or anyone)' : ''}',
                style: TextStyle(color: m.fg),
              ),
          ],
          if (org.templates.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              'Templates: ${org.templates.join(', ')}',
              style: TextStyle(color: m.muted),
            ),
          ],
        ],
      ),
    );
  }
}

class _JoinedTile extends StatelessWidget {
  const _JoinedTile(this.j);
  final OrgJoined j;

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final perms = j.perms.isEmpty ? '' : '  [${j.perms.join(', ')}]';
    return ListTile(
      contentPadding: EdgeInsets.zero,
      leading: Icon(Icons.link, color: m.accent),
      title: Text(j.name),
      subtitle: Text(
        '${j.slug} · ${j.status} as ${j.role} at …${j.node}$perms',
        style: TextStyle(color: m.muted),
      ),
    );
  }
}
