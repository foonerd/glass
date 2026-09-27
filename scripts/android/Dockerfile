# A Linux image that builds the display for Android: the pinned Rust
# toolchain with the Android targets and cargo-ndk, the Android SDK with
# its NDK, build tools and platform, a JDK and Gradle for the app, and the
# emulator with an x86_64 system image for a headless test:
#
#   docker build -t glass-android scripts/android
#   docker run --rm -v "$PWD:/glass" -w /glass glass-android scripts/ship-android.sh
FROM rust:bookworm
ARG RUST_VERSION=1.98.1
ARG CMDLINE_TOOLS=commandlinetools-linux-15859902_latest.zip
ARG CMDLINE_TOOLS_SHA256=4e4c464f145a7512b57d088ac6c278c03c9eea610886b35a5e0804e74eedf583
ARG GRADLE_VERSION=8.10.2
ENV ANDROID_HOME=/opt/android-sdk \
    ANDROID_SDK_ROOT=/opt/android-sdk \
    ANDROID_NDK_HOME=/opt/android-sdk/ndk/27.2.12479018 \
    GRADLE_HOME=/opt/gradle \
    PATH=/opt/gradle/bin:/opt/android-sdk/cmdline-tools/latest/bin:/opt/android-sdk/platform-tools:/opt/android-sdk/emulator:/usr/local/cargo/bin:$PATH
RUN apt-get update \
  && apt-get install -y --no-install-recommends openjdk-17-jdk-headless unzip zip libpulse0 libgl1 libnss3 libxcomposite1 libxcursor1 libxi6 libxtst6 libxrandr2 libasound2 libxdamage1 libxkbfile1 \
  && rm -rf /var/lib/apt/lists/*
RUN mkdir -p "$ANDROID_HOME/cmdline-tools" \
  && curl -fsSL -o /tmp/tools.zip "https://dl.google.com/android/repository/$CMDLINE_TOOLS" \
  && echo "$CMDLINE_TOOLS_SHA256  /tmp/tools.zip" | sha256sum -c - \
  && unzip -q /tmp/tools.zip -d "$ANDROID_HOME/cmdline-tools" \
  && mv "$ANDROID_HOME/cmdline-tools/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest" \
  && rm /tmp/tools.zip \
  && yes | sdkmanager --licenses >/dev/null \
  && sdkmanager "platform-tools" "platforms;android-35" "build-tools;35.0.0" "ndk;27.2.12479018" "emulator" "system-images;android-35;google_apis;x86_64" \
  && chmod -R a+rX "$ANDROID_HOME"
RUN curl -fsSL -o /tmp/gradle.zip "https://services.gradle.org/distributions/gradle-$GRADLE_VERSION-bin.zip" \
  && unzip -q /tmp/gradle.zip -d /opt && mv "/opt/gradle-$GRADLE_VERSION" /opt/gradle && rm /tmp/gradle.zip
RUN rustup toolchain install "$RUST_VERSION" --profile minimal \
  && rustup target add --toolchain "$RUST_VERSION" aarch64-linux-android armv7-linux-androideabi x86_64-linux-android \
  && cargo install cargo-ndk --locked \
  && chmod -R a+rX /usr/local/cargo /usr/local/rustup
