module.exports = {
  dependency: {
    platforms: {
      android: {
        sourceDir: "./android",
        componentDescriptors: [],
        packageImportPath:
          "import org.xmtp.reactnative.XmtpReactNativePackage;",
        packageInstance: "new XmtpReactNativePackage()",
      },
    },
  },
};
