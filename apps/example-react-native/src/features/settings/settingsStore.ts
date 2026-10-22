import { SettingsModel } from "@/core/settingsModel.ts";
import { sessionController } from "@/session";

/** The Settings route store, wired to the session controller. */
export function createSettingsModel(): SettingsModel {
  return new SettingsModel({
    signOut: () => sessionController.signOut(),
    deleteLocalData: () => sessionController.deleteLocalData(),
    continueReset: () => sessionController.continueReset(),
    setAppearance: (value) => sessionController.setAppearance(value),
  });
}
