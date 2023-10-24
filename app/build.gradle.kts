plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.mozilla.rust-android-gradle.rust-android")
}

android {
    // Don't have the matching NDK for AGP installed: https://developer.android.com/studio/projects/install-ndk#default-ndk-per-agp
    ndkVersion = "26.1.10909125"
    namespace = "rust.androidvulkaninterop"
    compileSdk = 34

    defaultConfig {
        applicationId = "rust.androidvulkaninterop"
        minSdk = 28 // Should remain in sync with the ndk api-level-xx feature in Rust
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro"
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

cargo {
    module = "../android_vulkan_interop"
    libname = "android_vulkan_interop"
    targets = listOf("arm64")
}

project.afterEvaluate {
    tasks.withType(com.nishtahir.CargoBuildTask::class)
        .forEach { buildTask ->
            tasks.withType(com.android.build.gradle.tasks.MergeSourceSetFolders::class)
                .configureEach {
                    this.inputs.dir(
                        layout.buildDirectory.dir("rustJniLibs" + File.separatorChar + buildTask.toolchain!!.folder)
                    )
                    this.dependsOn(buildTask)
                }
        }
}