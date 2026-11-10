/// Orgs: `mnnn org list` / `mnnn org show` plus create, edit, join, members,
/// roles, resources and hosting. The store opens for writes after unlock;
/// host certificates and invite private material stay in Rust (tokens the TUI
/// already shows are displayed so the other node can join).
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';
import 'orgs_dialogs.dart';
import 'shared_actions.dart';
import 'orgs_rest.dart';
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
                key: const ValueKey('orgs-list'),
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
                children: [
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      FilledButton(
                        key: const ValueKey('org-new'),
                        onPressed: () => showOrgCreateDialog(context, ref),
                        child: const Text('New organization'),
                      ),
                      OutlinedButton(
                        key: const ValueKey('org-join'),
                        onPressed: () => showOrgJoinDialog(context, ref),
                        child: const Text('Join'),
                      ),
                      OutlinedButton(
                        key: const ValueKey('org-apply'),
                        onPressed: () => showOrgApplyDialog(context, ref),
                        child: const Text('Apply'),
                      ),
                    ],
                  ),
                  const SizedBox(height: 12),
                  _HostBar(),
                  const SizedBox(height: 16),
                  if (empty)
                    Text(
                      'No organizations yet. New organization creates one here; Join uses an invitation; Apply asks another node.',
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
                        'None yet: Apply to an org on another node, or Join with an invitation.',
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
                ],
              );
            },
          ),
    );
  }
}

class _HostBar extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(orgHostStatusProvider)
        .when(
          loading: () => const SizedBox.shrink(),
          error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
          data: (st) {
            if (st == null) return const SizedBox.shrink();
            final status = st.running
                ? 'Site at ${st.url}${st.orgs.isEmpty ? '' : ' · ${st.orgs.join(', ')}'}'
                : 'Site not running';
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(status, style: TextStyle(color: m.muted)),
                if (st.certSource.isNotEmpty)
                  Text(
                    'certificate: ${st.certSource}',
                    style: TextStyle(color: m.muted),
                  ),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    OutlinedButton(
                      key: const ValueKey('org-host-start'),
                      onPressed: () => orgHostStart(context, ref),
                      child: const Text('Start site'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('org-host-stop'),
                      onPressed: () => orgHostStop(context, ref),
                      child: const Text('Stop site'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('org-announce-open'),
                      onPressed: () => showOrgAnnounceDialog(context, ref),
                      child: const Text('Announce…'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('org-announced-open'),
                      onPressed: () => showAnnouncedSitesDialog(context, ref),
                      child: const Text('Announced sites…'),
                    ),
                    FilterChip(
                      key: const ValueKey('org-host-autostart'),
                      label: Text(
                        st.autostart ? 'Autostart on' : 'Autostart off',
                      ),
                      selected: st.autostart,
                      onSelected: (v) => orgHostAutostart(context, ref, v),
                    ),
                  ],
                ),
              ],
            );
          },
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

Widget _btn(String key, String label, VoidCallback onPressed) => Padding(
  padding: const EdgeInsets.only(right: 8, bottom: 8),
  child: OutlinedButton(
    key: ValueKey<String>(key),
    onPressed: onPressed,
    child: Text(label),
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

class _MineDetail extends ConsumerWidget {
  const _MineDetail(this.org);
  final OrgInfo org;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
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
          const SizedBox(height: 8),
          Wrap(
            children: [
              _btn(
                'org-edit-${org.slug}',
                'Edit',
                () => showOrgEditDialog(context, ref, org),
              ),
              _btn(
                'org-invite-${org.slug}',
                'Invite',
                () => showOrgInviteDialog(context, ref, org),
              ),
              _btn(
                'org-remove-${org.slug}',
                'Remove',
                () => confirmOrgRemove(context, ref, org),
              ),
            ],
          ),
          if (org.roles.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              'Roles',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
            ),
            for (final r in org.roles)
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '  ${r.name}  rank ${r.rank}  ${r.perms.join(' ')}',
                    style: TextStyle(color: m.fg),
                  ),
                  Wrap(
                    children: [
                      _btn(
                        'org-role-edit-${r.name}',
                        'Permissions',
                        () => showOrgRolePermsDialog(context, ref, org, r),
                      ),
                      _btn(
                        'org-role-apply-${r.name}',
                        'Default for apply',
                        () async {
                          try {
                            await ref
                                .read(apiProvider)
                                .orgEdit(org.slug, applyRole: r.name);
                            refreshOrgs(ref);
                          } catch (e) {
                            if (context.mounted) {
                              ScaffoldMessenger.of(context)
                                  .showSnackBar(SnackBar(content: Text('$e')));
                            }
                          }
                        },
                      ),
                      _btn(
                        'org-role-invite-${r.name}',
                        'Default for invite',
                        () async {
                          try {
                            await ref
                                .read(apiProvider)
                                .orgEdit(org.slug, inviteRole: r.name);
                            refreshOrgs(ref);
                          } catch (e) {
                            if (context.mounted) {
                              ScaffoldMessenger.of(context)
                                  .showSnackBar(SnackBar(content: Text('$e')));
                            }
                          }
                        },
                      ),
                      if (!r.name.contains('owner'))
                        _btn(
                          'org-role-remove-${r.name}',
                          'Remove role',
                          () async {
                            if (!await confirmOrg(
                              context,
                              'Remove role ${r.name}?',
                            )) {
                              return;
                            }
                            if (!context.mounted) {
                              return;
                            }
                            try {
                              final msg = await ref
                                  .read(apiProvider)
                                  .orgRoleRemove(org.slug, r.name);
                              refreshOrgs(ref);
                              if (context.mounted) {
                                ScaffoldMessenger.of(context)
                                    .showSnackBar(SnackBar(content: Text(msg)));
                              }
                            } catch (e) {
                              if (context.mounted) {
                                ScaffoldMessenger.of(
                                  context,
                                ).showSnackBar(SnackBar(content: Text('$e')));
                              }
                            }
                          },
                        ),
                    ],
                  ),
                ],
              ),
            _btn(
              'org-role-new-${org.slug}',
              'New role',
              () => showOrgNewRoleDialog(context, ref, org),
            ),
          ],
          if (org.memberList.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              'Members',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
            ),
            for (final x in org.memberList)
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '  ${x.title}  ${x.role}  ${x.status}${x.note.isEmpty ? '' : '  ${x.note}'}',
                    style: TextStyle(color: m.fg),
                  ),
                  Wrap(
                    children: [
                      if (x.status == 'applied') ...[
                        _btn('org-approve-${x.title}', 'Approve', () async {
                          try {
                            final msg = await ref
                                .read(apiProvider)
                                .orgMemberApprove(org.slug, x.title);
                            refreshOrgs(ref);
                            if (context.mounted) {
                              ScaffoldMessenger.of(context)
                                  .showSnackBar(SnackBar(content: Text(msg)));
                            }
                          } catch (e) {
                            if (context.mounted) {
                              ScaffoldMessenger.of(context)
                                  .showSnackBar(SnackBar(content: Text('$e')));
                            }
                          }
                        }),
                        _btn('org-reject-${x.title}', 'Reject', () async {
                          try {
                            final msg = await ref
                                .read(apiProvider)
                                .orgMemberReject(org.slug, x.title);
                            refreshOrgs(ref);
                            if (context.mounted) {
                              ScaffoldMessenger.of(context)
                                  .showSnackBar(SnackBar(content: Text(msg)));
                            }
                          } catch (e) {
                            if (context.mounted) {
                              ScaffoldMessenger.of(context)
                                  .showSnackBar(SnackBar(content: Text('$e')));
                            }
                          }
                        }),
                      ],
                      _btn(
                        'org-member-role-${x.title}',
                        'Role',
                        () => showOrgMemberRoleDialog(context, ref, org, x),
                      ),
                      _btn(
                        'org-kick-${x.title}',
                        'Kick',
                        () => showOrgKickDialog(context, ref, org, x),
                      ),
                    ],
                  ),
                ],
              ),
          ],
          if (org.resources.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(
              'Resources',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
            ),
            for (final r in org.resources)
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '  ${r.kind}  ${r.name}  ${r.state}  published: ${r.published}',
                    style: TextStyle(color: m.fg),
                  ),
                  Wrap(
                    children: [
                      _btn('org-res-pub-${r.name}', 'Publish', () async {
                        try {
                          final msg = await ref
                              .read(apiProvider)
                              .orgResourcePublish(org.slug, r.name);
                          refreshOrgs(ref);
                          if (context.mounted) {
                            ScaffoldMessenger.of(context)
                                .showSnackBar(SnackBar(content: Text(msg)));
                          }
                        } catch (e) {
                          if (context.mounted) {
                            ScaffoldMessenger.of(context)
                                .showSnackBar(SnackBar(content: Text('$e')));
                          }
                        }
                      }),
                      _btn('org-res-arch-${r.name}', 'Archive', () async {
                        try {
                          final msg = await ref
                              .read(apiProvider)
                              .orgResourceArchive(org.slug, r.name);
                          refreshOrgs(ref);
                          if (context.mounted) {
                            ScaffoldMessenger.of(context)
                                .showSnackBar(SnackBar(content: Text(msg)));
                          }
                        } catch (e) {
                          if (context.mounted) {
                            ScaffoldMessenger.of(context)
                                .showSnackBar(SnackBar(content: Text('$e')));
                          }
                        }
                      }),
                      _btn('org-res-rel-${r.name}', 'Release', () async {
                        if (!await confirmOrg(
                          context,
                          '${r.name} is no longer ${org.name}\'s? (it stays here)',
                        )) {
                          return;
                        }
                        if (!context.mounted) {
                          return;
                        }
                        try {
                          final msg = await ref
                              .read(apiProvider)
                              .orgResourceRelease(org.slug, r.name);
                          refreshOrgs(ref);
                          if (context.mounted) {
                            ScaffoldMessenger.of(context)
                                .showSnackBar(SnackBar(content: Text(msg)));
                          }
                        } catch (e) {
                          if (context.mounted) {
                            ScaffoldMessenger.of(context)
                                .showSnackBar(SnackBar(content: Text('$e')));
                          }
                        }
                      }),
                    ],
                  ),
                ],
              ),
          ],
          _btn(
            'org-res-add-${org.slug}',
            'Add resource',
            () => showOrgAddResourceDialog(context, ref, org),
          ),
          OrgMineRest(org),
        ],
      ),
    );
  }
}

class _JoinedTile extends ConsumerWidget {
  const _JoinedTile(this.j);
  final OrgJoined j;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final perms = j.perms.isEmpty ? '' : '  [${j.perms.join(', ')}]';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        ListTile(
          contentPadding: EdgeInsets.zero,
          leading: Icon(Icons.link, color: m.accent),
          title: Text(j.name),
          subtitle: Text(
            '${j.slug} · ${j.status} as ${j.role} at …${j.node}$perms',
            style: TextStyle(color: m.muted),
          ),
        ),
        Padding(
          padding: const EdgeInsets.only(left: 56),
          child: Wrap(
            children: [
              _btn(
                'org-sync-${j.slug}',
                'Sync',
                () => orgSyncJoined(context, ref, j),
              ),
              _btn(
                'org-leave-${j.slug}',
                'Leave',
                () => confirmOrgLeave(context, ref, j),
              ),
              OrgJoinedRest(j),
            ],
          ),
        ),
      ],
    );
  }
}
