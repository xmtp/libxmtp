import { AppRegistry } from "react-native";

import { name } from "./app.json";
import App from "./harness/App";
AppRegistry.registerComponent(name, () => App);

import { runBackgroundTask } from "./harness/background";
AppRegistry.registerHeadlessTask(
  "XmtpBackgroundFixture",
  () => runBackgroundTask,
);
