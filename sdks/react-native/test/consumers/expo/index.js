import { AppRegistry } from "react-native";

import App from "./harness/App";
AppRegistry.registerComponent("main", () => App);

import { runBackgroundTask } from "./harness/background";
AppRegistry.registerHeadlessTask(
  "XmtpBackgroundFixture",
  () => runBackgroundTask,
);
