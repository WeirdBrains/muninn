/// Orgs write dialogs (`mnnn org create|edit|join|apply|invite|host`).
/// Invite tokens are shown the same way as the TUI; host certificates stay in Rust.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';

void _snack(BuildContext context, String msg, {bool error = false}) {
  if (!context.mounted) {
    return;
  }
  final m = context.muninn;
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(content: Text(msg), backgroundColor: error ? m.error : m.success),
  );
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
        width: 420,
        child: SelectableText(body, key: const ValueKey('org-result-text')),
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

Future<bool> confirmOrg(BuildContext context, String question) async {
  final m = context.muninn;
  return await showDialog<bool>(
        context: context,
        builder: (ctx) => AlertDialog(
          backgroundColor: m.bg,
          title: Text(question, style: TextStyle(color: m.header)),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('No'),
            ),
            TextButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('Yes'),
            ),
          ],
        ),
      ) ==
      true;
}

Future<void> orgHostStart(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.orgs;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  try {
    final st = await ref.read(apiProvider).orgHostStart();
    refreshOrgs(ref);
    if (!context.mounted) {
      return;
    }
    _snack(
      context,
      st.running ? 'serving at ${st.url}' : 'the site is not running',
    );
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> orgHostStop(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.orgs;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  await _after(context, ref, ref.read(apiProvider).orgHostStop());
}

Future<void> orgHostAutostart(
  BuildContext context,
  WidgetRef ref,
  bool on,
) async {
  if (!ref.read(unlockedProvider)) {
    return;
  }
  await _after(context, ref, ref.read(apiProvider).orgHostAutostart(on));
}

Future<void> showOrgCreateDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.orgs;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final created = await showDialog<OrgInfo>(
    context: context,
    builder: (ctx) => const _CreateDialog(),
  );
  if (created == null) {
    return;
  }
  try {
    final org = await ref
        .read(apiProvider)
        .orgCreate(
          created.name,
          slug: created.slug.isEmpty ? null : created.slug,
          description: created.description,
          publicOrg: created.visibility == 'public',
          openAccess: created.access == 'open',
        );
    refreshOrgs(ref);
    if (!context.mounted) {
      return;
    }
    _snack(
      context,
      'created ${org.name} (${org.slug}): ${org.visibility}, ${org.access}',
    );
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> showOrgEditDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final edited = await showDialog<OrgInfo>(
    context: context,
    builder: (ctx) => _EditDialog(org: org),
  );
  if (edited == null) {
    return;
  }
  try {
    final o = await ref
        .read(apiProvider)
        .orgEdit(
          org.slug,
          name: edited.name,
          slug: edited.slug,
          description: edited.description,
          visibility: edited.visibility,
          access: edited.access,
          hosted: edited.hosted,
          memberAuth: edited.memberAuth,
          gated: edited.gated,
          template: edited.template,
        );
    refreshOrgs(ref);
    if (!context.mounted) {
      return;
    }
    _snack(
      context,
      '${o.name} (${o.slug}): ${o.visibility}, ${o.access}${o.hosted ? ", hosted" : ""}',
    );
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> confirmOrgRemove(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  if (!await confirmOrg(
    context,
    'Remove ${org.name}? Its resources stay here',
  )) {
    return;
  }
  if (!context.mounted) {
    return;
  }
  await _after(context, ref, ref.read(apiProvider).orgRemove(org.slug));
}

Future<void> showOrgJoinDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.orgs;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final token = await showDialog<String>(
    context: context,
    builder: (ctx) => const _FieldDialog(
      title: 'Join',
      hint: 'Invitation token or link',
      keyName: 'org-join-token',
    ),
  );
  if (token == null || token.trim().isEmpty || !context.mounted) {
    return;
  }
  try {
    final msg = await ref.read(apiProvider).orgJoin(token.trim());
    refreshOrgs(ref);
    if (!context.mounted) {
      return;
    }
    await _showText(context, 'Joined', msg);
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> showOrgApplyDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.orgs;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final pair = await showDialog<(String, String, String)>(
    context: context,
    builder: (ctx) => const _ApplyDialog(),
  );
  if (pair == null || !context.mounted) {
    return;
  }
  await _after(
    context,
    ref,
    ref.read(apiProvider).orgApply(pair.$1, pair.$2, note: pair.$3),
  );
}

Future<void> confirmOrgLeave(
  BuildContext context,
  WidgetRef ref,
  OrgJoined j,
) async {
  if (!await confirmOrg(context, 'Leave ${j.name}?')) {
    return;
  }
  if (!context.mounted) {
    return;
  }
  await _after(context, ref, ref.read(apiProvider).orgLeave(j.slug));
}

Future<void> orgSyncJoined(
  BuildContext context,
  WidgetRef ref,
  OrgJoined j,
) async {
  await _after(context, ref, ref.read(apiProvider).orgSync(j.slug));
}

Future<void> showOrgInviteDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final to = await showDialog<String>(
    context: context,
    builder: (ctx) => const _FieldDialog(
      title: 'Invite',
      hint: 'Their node …/p2p/ID or peer; empty: just a token',
      keyName: 'org-invite-to',
      allowEmpty: true,
    ),
  );
  if (to == null || !context.mounted) {
    return;
  }
  try {
    final msg = await ref
        .read(apiProvider)
        .orgMemberInvite(org.slug, to: to.trim().isEmpty ? null : to.trim());
    refreshOrgs(ref);
    if (!context.mounted) {
      return;
    }
    await _showText(context, 'Invitation', msg);
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> showOrgMemberRoleDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
  OrgMemberInfo member,
) async {
  final role = await showDialog<String>(
    context: context,
    builder: (ctx) => _FieldDialog(
      title: 'Role for ${member.title}',
      hint: 'Role name',
      initial: member.role == '-' ? '' : member.role,
      keyName: 'org-member-role',
    ),
  );
  if (role == null || role.trim().isEmpty || !context.mounted) {
    return;
  }
  await _after(
    context,
    ref,
    ref.read(apiProvider).orgMemberRole(org.slug, member.title, role.trim()),
  );
}

Future<void> showOrgKickDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
  OrgMemberInfo member,
) async {
  final left = await showDialog<String>(
    context: context,
    builder: (ctx) => _FieldDialog(
      title: 'Remove ${member.title}',
      hint: 'What they gave: keep, archive or publish',
      initial: 'keep',
      keyName: 'org-kick-leftovers',
    ),
  );
  if (left == null || !context.mounted) {
    return;
  }
  await _after(
    context,
    ref,
    ref
        .read(apiProvider)
        .orgMemberKick(
          org.slug,
          member.title,
          leftovers: left.trim().isEmpty ? 'keep' : left.trim(),
        ),
  );
}

Future<void> showOrgNewRoleDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  final v = await showDialog<String>(
    context: context,
    builder: (ctx) => const _FieldDialog(
      title: 'New role',
      hint: 'name [rank] perms…',
      keyName: 'org-new-role',
    ),
  );
  if (v == null || v.trim().isEmpty || !context.mounted) {
    return;
  }
  final w = v.trim().split(RegExp(r'\s+'));
  final name = w.first;
  final parsed = w.length > 1 ? int.tryParse(w[1]) : null;
  final rank = parsed ?? 30;
  final perms = w.skip(parsed == null ? 1 : 2).toList();
  try {
    final r = await ref
        .read(apiProvider)
        .orgRoleCreate(org.slug, name, rank: rank, perms: perms);
    refreshOrgs(ref);
    if (!context.mounted) {
      return;
    }
    _snack(context, 'role ${r.name} (rank ${r.rank}): ${r.perms.join(' ')}');
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> showOrgRolePermsDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
  OrgRoleInfo role,
) async {
  final v = await showDialog<String>(
    context: context,
    builder: (ctx) => _FieldDialog(
      title: 'Permissions of ${role.name}',
      hint: 'perms…',
      initial: role.perms.join(' '),
      keyName: 'org-role-perms',
    ),
  );
  if (v == null || !context.mounted) {
    return;
  }
  try {
    final r = await ref
        .read(apiProvider)
        .orgRoleEdit(
          org.slug,
          role.name,
          perms: v.trim().isEmpty ? <String>[] : v.trim().split(RegExp(r'\s+')),
        );
    refreshOrgs(ref);
    if (!context.mounted) {
      return;
    }
    _snack(context, 'role ${r.name}: ${r.perms.join(' ')}');
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> showOrgAddResourceDialog(
  BuildContext context,
  WidgetRef ref,
  OrgInfo org,
) async {
  final kindCtl = TextEditingController(text: 'journal');
  final nameCtl = TextEditingController();
  final v = await showDialog<String>(
    context: context,
    builder: (ctx) {
      final m = ctx.muninn;
      return AlertDialog(
        backgroundColor: m.bg,
        title: Text('Give the org', style: TextStyle(color: m.header)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: kindCtl,
              decoration: const InputDecoration(labelText: 'Kind'),
            ),
            TextField(
              key: const ValueKey('org-add-resource'),
              controller: nameCtl,
              decoration: const InputDecoration(labelText: 'Name'),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () async {
              final k = kindCtl.text.trim();
              final n = nameCtl.text.trim();
              if (k.isEmpty || n.isEmpty) return;
              try {
                final created = await ref
                    .read(apiProvider)
                    .shareCreateItem(k, n);
                if (ctx.mounted) {
                  Navigator.pop(
                    ctx,
                    '${created.createdItemKind} ${created.createdItemName}',
                  );
                }
              } catch (e) {
                if (context.mounted) _snack(context, '$e', error: true);
              }
            },
            child: const Text('+ New…'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(
              ctx,
              '${kindCtl.text.trim()} ${nameCtl.text.trim()}',
            ),
            child: const Text('Add'),
          ),
        ],
      );
    },
  );
  kindCtl.dispose();
  nameCtl.dispose();
  if (v == null || v.trim().isEmpty || !context.mounted) {
    return;
  }
  final parts = v.trim().split(RegExp(r'\s+'));
  final kind = parts.first;
  final name = parts.skip(1).join(' ');
  await _after(
    context,
    ref,
    ref.read(apiProvider).orgResourceAdd(org.slug, kind, name),
  );
}

class _FieldDialog extends StatefulWidget {
  const _FieldDialog({
    required this.title,
    required this.hint,
    this.initial = '',
    this.keyName,
    this.allowEmpty = false,
  });
  final String title;
  final String hint;
  final String initial;
  final String? keyName;
  final bool allowEmpty;

  @override
  State<_FieldDialog> createState() => _FieldDialogState();
}

class _FieldDialogState extends State<_FieldDialog> {
  late final TextEditingController _c = TextEditingController(
    text: widget.initial,
  );

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  void _ok() {
    final v = _c.text;
    if (!widget.allowEmpty && v.trim().isEmpty) {
      return;
    }
    Navigator.pop(context, v);
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(widget.title, style: TextStyle(color: m.header)),
      content: TextField(
        key: widget.keyName == null ? null : ValueKey<String>(widget.keyName!),
        controller: _c,
        autofocus: true,
        decoration: InputDecoration(hintText: widget.hint),
        onSubmitted: (_) => _ok(),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        TextButton(onPressed: _ok, child: const Text('OK')),
      ],
    );
  }
}

class _CreateDialog extends StatefulWidget {
  const _CreateDialog();

  @override
  State<_CreateDialog> createState() => _CreateDialogState();
}

class _CreateDialogState extends State<_CreateDialog> {
  final _name = TextEditingController();
  final _slug = TextEditingController();
  final _desc = TextEditingController();
  bool _public = false;
  bool _open = false;

  @override
  void dispose() {
    _name.dispose();
    _slug.dispose();
    _desc.dispose();
    super.dispose();
  }

  void _ok() {
    if (_name.text.trim().isEmpty) {
      return;
    }
    Navigator.pop(
      context,
      OrgInfo(
        id: '',
        shortId: '',
        name: _name.text.trim(),
        slug: _slug.text.trim(),
        description: _desc.text.trim(),
        visibility: _public ? 'public' : 'private',
        access: _open ? 'open' : 'members',
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('New organization', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('org-create-name'),
              controller: _name,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Name'),
              onSubmitted: (_) => _ok(),
            ),
            TextField(
              controller: _slug,
              decoration: const InputDecoration(labelText: 'Slug (optional)'),
            ),
            TextField(
              controller: _desc,
              decoration: const InputDecoration(labelText: 'Description'),
            ),
            CheckboxListTile(
              contentPadding: EdgeInsets.zero,
              title: const Text('Public'),
              value: _public,
              onChanged: (v) => setState(() => _public = v ?? false),
            ),
            CheckboxListTile(
              contentPadding: EdgeInsets.zero,
              title: const Text('Open'),
              value: _open,
              onChanged: (v) => setState(() => _open = v ?? false),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        TextButton(onPressed: _ok, child: const Text('Create')),
      ],
    );
  }
}

class _EditDialog extends StatefulWidget {
  const _EditDialog({required this.org});
  final OrgInfo org;

  @override
  State<_EditDialog> createState() => _EditDialogState();
}

class _EditDialogState extends State<_EditDialog> {
  late final TextEditingController _name = TextEditingController(
    text: widget.org.name,
  );
  late final TextEditingController _slug = TextEditingController(
    text: widget.org.slug,
  );
  late final TextEditingController _desc = TextEditingController(
    text: widget.org.description,
  );
  late String _visibility = widget.org.visibility;
  late String _access = widget.org.access;
  late bool _hosted = widget.org.hosted;
  late bool _memberAuth = widget.org.memberAuth;
  late bool _gated = widget.org.gated;

  @override
  void dispose() {
    _name.dispose();
    _slug.dispose();
    _desc.dispose();
    super.dispose();
  }

  void _ok() {
    if (_name.text.trim().isEmpty) {
      return;
    }
    Navigator.pop(
      context,
      OrgInfo(
        id: widget.org.id,
        shortId: widget.org.shortId,
        name: _name.text.trim(),
        slug: _slug.text.trim(),
        description: _desc.text.trim(),
        visibility: _visibility,
        access: _access,
        hosted: _hosted,
        memberAuth: _memberAuth,
        gated: _gated,
        template: widget.org.template,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Edit ${widget.org.name}', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                key: const ValueKey('org-edit-name'),
                controller: _name,
                decoration: const InputDecoration(labelText: 'Name'),
              ),
              TextField(
                controller: _slug,
                decoration: const InputDecoration(labelText: 'Slug'),
              ),
              TextField(
                controller: _desc,
                decoration: const InputDecoration(labelText: 'Description'),
              ),
              DropdownButtonFormField<String>(
                initialValue: _visibility,
                decoration: const InputDecoration(labelText: 'Visibility'),
                items: const [
                  DropdownMenuItem(value: 'private', child: Text('private')),
                  DropdownMenuItem(value: 'public', child: Text('public')),
                ],
                onChanged: (v) =>
                    setState(() => _visibility = v ?? _visibility),
              ),
              DropdownButtonFormField<String>(
                initialValue: _access,
                decoration: const InputDecoration(labelText: 'Access'),
                items: const [
                  DropdownMenuItem(value: 'members', child: Text('members')),
                  DropdownMenuItem(value: 'open', child: Text('open')),
                ],
                onChanged: (v) => setState(() => _access = v ?? _access),
              ),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                title: const Text('Hosted'),
                value: _hosted,
                onChanged: (v) => setState(() => _hosted = v),
              ),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                title: const Text('Member sign-in'),
                value: _memberAuth,
                onChanged: (v) => setState(() => _memberAuth = v),
              ),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                title: const Text('Gated'),
                value: _gated,
                onChanged: (v) => setState(() => _gated = v),
              ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        TextButton(onPressed: _ok, child: const Text('Save')),
      ],
    );
  }
}

class _ApplyDialog extends StatefulWidget {
  const _ApplyDialog();

  @override
  State<_ApplyDialog> createState() => _ApplyDialogState();
}

class _ApplyDialogState extends State<_ApplyDialog> {
  final _target = TextEditingController();
  final _org = TextEditingController();
  final _note = TextEditingController();

  @override
  void dispose() {
    _target.dispose();
    _org.dispose();
    _note.dispose();
    super.dispose();
  }

  void _ok() {
    if (_target.text.trim().isEmpty || _org.text.trim().isEmpty) {
      return;
    }
    Navigator.pop(context, (
      _target.text.trim(),
      _org.text.trim(),
      _note.text.trim(),
    ));
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Apply', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('org-apply-target'),
              controller: _target,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Node'),
            ),
            TextField(
              key: const ValueKey('org-apply-org'),
              controller: _org,
              decoration: const InputDecoration(labelText: 'Organization'),
            ),
            TextField(
              controller: _note,
              decoration: const InputDecoration(labelText: 'Note'),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        TextButton(onPressed: _ok, child: const Text('Apply')),
      ],
    );
  }
}
