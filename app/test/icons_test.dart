import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

/// PNG width and height from the IHDR chunk.
(int, int) pngSize(String path) {
  final b = ByteData.sublistView(File(path).readAsBytesSync());
  return (b.getUint32(16), b.getUint32(20));
}

void main() {
  test('launcher icon sources are square renders of the raven', () {
    expect(pngSize('assets/icon/muninn-1024.png'), (1024, 1024));
    expect(pngSize('assets/icon/muninn-mark-1024.png'), (1024, 1024));
    expect(pngSize('assets/icon/muninn-256.png'), (256, 256));
    expect(File('assets/logo/muninn-raven.svg').readAsStringSync(), File('../assets/icons/muninn.svg').readAsStringSync(),
        reason: 'the app logo and the installer icon share one source');
  });

  test('every platform has generated launcher icons', () {
    for (final p in [
      'android/app/src/main/res/mipmap-xxxhdpi/ic_launcher.png',
      'android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml',
      'ios/Runner/Assets.xcassets/AppIcon.appiconset/Icon-App-1024x1024@1x.png',
      'macos/Runner/Assets.xcassets/AppIcon.appiconset/app_icon_1024.png',
      'windows/runner/resources/app_icon.ico',
    ]) {
      expect(File(p).existsSync(), isTrue, reason: p);
    }
    expect(pngSize('ios/Runner/Assets.xcassets/AppIcon.appiconset/Icon-App-1024x1024@1x.png'), (1024, 1024));
  });
}
