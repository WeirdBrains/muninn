Synthetic key files written by Muninn's own keyring at f123f855 (before `weirdnet-keys`
existed) into a temporary home: password `fixture-pw`, export password `fixture-export-pw`,
shard password `fixture-shard-pw`, service key rotated once (v1 retired, v2 current). They
protect nothing; `compat_tests::keys_written_before_the_extraction_still_open` checks the
extracted code opens every one of them and writes them back byte for byte.
