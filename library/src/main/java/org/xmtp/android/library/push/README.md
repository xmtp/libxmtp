# Enable the example app to send push notifications

You can use a Firebase Cloud Messaging server and an example push notification server to enable the `xmtp-android` example app to send push notifications.

Perform this setup to understand how you might want to enable push notifications for your own app built with the `xmtp-android` SDK.

## Set up a Firebase Cloud Messaging server

For this tutorial, we'll use [Firebase Cloud Messaging](https://console.firebase.google.com/) (FCM) as a convenient way to set up a messaging server.

1. Create an FCM project.

2. Add the example app to the FCM project. This generates a `google-services.json` file that you need in subsequent steps.

3. Add the `google-services.json` file to the example app's project as described in the FCM project creation process.

4. Generate FCM credentials, which you need to run the example notification server. To do this, from the FCM dashboard, click the gear icon next to **Project Overview** and select **Project settings**. Select **Service accounts**. Select **Go** and click **Generate new private key**. 

## Run an example notification server

Now that you have an FCM server set up, take a look at the [export-kotlin-proto-code](https://github.com/xmtp/example-notification-server-go/tree/np/export-kotlin-proto-code) branch in the `example-notifications-server-go` repo. 

This example branch can serve as the basis for what you might want to provide for your own notification server. The branch also demonstrates how to generate the proto code if you decide to perform these tasks for your own app. This proto code from the example notification server has already been generated in the `xmtp-android` example app.

**To run a notification server based on the example branch:**

1. Clone the [example-notification-server-go](https://github.com/xmtp/example-notification-server-go) repo.

2. Complete the steps in [Local Setup](https://github.com/xmtp/example-notification-server-go/blob/np/export-kotlin-proto-code/README.md#local-setup).

3. Get the FCM project ID and `google-services.json` file you created earlier and run:

    ```bash
    dev/run \                                                                     
    --xmtp-listener-tls \
    --xmtp-listener \
    --api \
    -x "grpc.production.xmtp.network:443:5556" \
    -d "postgres://postgres:xmtp@localhost:25432/postgres?sslmode=disable" \
    --fcm-enabled \
    --fcm-credentials-json=YOURFCMJSON \
    --fcm-project-id="YOURFCMPROJECTID"
    ```

4. You should now be able to see push notifications coming across the local network.

## Update the example app to send push notifications

1. Add your `google-services.json` file to the `example` folder, if you haven't already done it as a part of the FCM project creation process.

2. Uncomment `id 'com.google.gms.google-services'` in the example app's `build.gradle` file.

3. Uncomment the following code in the top level of the example app's `build.gradle` file:

    ```
    buildscript {
        repositories {
            google()
            mavenCentral()
        }
        dependencies {
            classpath 'com.google.gms:google-services:4.3.15'
        }
    }
    ```

4. Sync the gradle project.

5. Add the example notification server address to the example app's `MainActivity`. In this case, it should be `PushNotificationTokenManager.init(this, "10.0.2.2:8080")`.

6. Change the example app's environment to `XMTPEnvironment.PRODUCTION` in `Client.kt`.

7. Set up the example app to register the FCM token with the network and then subscribe each conversation to push notifications. For example:

    ```kotlin
    XMTPPush(context, "10.0.2.2:8080").register(token)
    ```

    ```kotlin
    XMTPPush(context, "10.0.2.2:8080").subscribe(conversations.map { it.topic })
    ```

    ```kotlin
    XMTPPush(context, "10.0.2.2:8080").unsubscribe(conversations.map { it.topic })
    ```
