# Push notifications setup

The app stores a push endpoint per installation. The server sends notifications only to users with no active messenger connection, which prevents a second alert while another Luma session is online.

## Android (FCM)

1. In Firebase, add an Android app with package name `com.coffeemain.luma` and download `google-services.json`.
2. Run `npm run tauri android init` from `luma-desktop` if the generated Android project is missing, then place the file at `luma-desktop/src-tauri/gen/android/app/google-services.json`.
3. In Firebase project settings, create a service account key and enable the Firebase Cloud Messaging API. Store the complete service-account JSON in the server's secret environment variable `FCM_SERVICE_ACCOUNT_JSON`. Do not commit the key.
4. Rebuild and install the Android app. Grant notification permission when prompted. The app registers its FCM token and refreshes it when Firebase rotates the token.

The Android Tauri plugin registers `FCMService` and configures Google Services during `tauri android init`. The generated Android project is ignored by Git, so the Firebase config file must be installed again on each development/build machine.

## Windows (WNS)

Windows push uses Windows Push Notification Services (WNS), not FCM. The server sender is implemented, but a Windows channel must first be registered by the installed app. Full background delivery requires a packaged app identity; the current Tauri bundle targets MSI/NSIS and does not produce MSIX.

To finish Windows push:

1. Create a multi-tenant Microsoft Entra app registration and configure the Windows App SDK Push Notifications integration for the app. Record the Tenant ID, Application (client) ID, service-principal Object ID, and MSIX package identity.
2. Package and sign Luma as MSIX, register the package identity with the Azure app as required by Microsoft's WNS setup, and add the Windows App SDK push-channel registration/activation path to the Windows app.
3. Register each WNS channel URI with `POST /api/push-token` using the user's bearer token and JSON `{ "token": "<channel-uri>", "platform": "windows" }`.
4. Set `WNS_TENANT_ID`, `WNS_CLIENT_ID`, and `WNS_CLIENT_SECRET` in the server's secret environment. Never commit the client secret.

The server removes expired WNS channels after WNS reports them as gone. The app's current Windows local notification path continues to work while the app process is running.

## Server behavior

Pushes are sent for `general`, `dev`, and `friends` rooms to other registered accounts. Direct-message room IDs use the existing `dm:<user-id>:<user-id>` format. The local-only `saved` room does not send pushes.
