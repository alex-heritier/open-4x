# Mobile launchers

These are build entry points; SDK builds/device runs must be checked on the platform toolchains.

Android uses Bevy NativeActivity and the APK configuration in the client manifest. Install Android SDK/NDK, the `aarch64-linux-android` Rust target, and `cargo-apk`, set the SDK/NDK variables required by that tool, then:

```sh
cargo apk run -p fourx-client --lib --no-default-features --features android
```

The APK bundles the contents of `assets/` at its asset root. This chooses NativeActivity so the starter can package without a separate Java/Gradle project. `cargo-apk` is a legacy packaging tool; a production Android release can replace it with a GameActivity Gradle wrapper while retaining the simulation/client split. Saves live in Android's internal app data. Browser play is also available on mobile devices supporting WebGL2.

For iOS, use macOS with Xcode, the `aarch64-apple-ios` Rust target, and XcodeGen. From the workspace:

```sh
cargo rustc -p fourx-client --lib --release --no-default-features --target aarch64-apple-ios --crate-type staticlib
cd platforms/ios
xcodegen generate
open Dawn.xcodeproj
```

Choose your signing team and an attached device in Xcode. The generated app links the Rust library, calls `fourx_ios_main`, and includes the `assets` folder. Signing credentials and generated Xcode build products are not stored in the repository. iOS assets resolve alongside the app executable and saves use the app Documents directory. Simulator targets require their own Rust artifact and project library path.
