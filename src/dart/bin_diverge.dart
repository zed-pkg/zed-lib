import 'dart:convert';
import 'dart:io';
import 'package:zed_interfaces/package_metadata.dart';
import 'package:zed_lib/zed_lib.dart';

void main() {
  for (final name in ['fuzz-version-resolution', 'fuzz-latest-stable']) {
    final doc = jsonDecode(File('../../conformance/cases/$name.json').readAsStringSync()) as Map<String, dynamic>;
    final isLatest = name.contains('latest');
    for (final c in (doc['cases'] as List).cast<Map<String, dynamic>>()) {
      final versions = (c['versions'] as List).cast<String>();
      final scheme = switch (c['scheme'] as String) {
        'calver' => VersionScheme.calver,
        'opaque' => VersionScheme.opaque,
        _ => VersionScheme.semver,
      };
      final meta = PackageMetadata(
        org: 'a', name: 'b', vcs: Vcs.git, repoUrl: 'u',
        latest: isLatest ? c['latest'] as String? : (c['latest'] as String? ?? (versions.isEmpty ? null : versions.last)),
        versions: versions, versionScheme: scheme);
      final expect = c['expect'] as Map<String, dynamic>;
      String got;
      if (isLatest) {
        got = 'version=${latestStable(meta)}';
      } else {
        try { got = 'version=${resolveVersion(meta, c['requirement'] as String)}'; }
        on ResolveException catch (e) { got = 'error=${e.kind.wire}'; }
      }
      final want = expect.containsKey('error') && expect['error'] != null
          ? 'error=${expect['error']}' : 'version=${expect['version']}';
      if (got != want) {
        print('${c['name']} scheme=${c['scheme']} req=${c['requirement']} versions=$versions');
        print('   rust=$want  dart=$got');
      }
    }
  }
}
