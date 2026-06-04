export {
  readPodspecVersion,
  writePodspecVersion,
  createPodspecManifestProvider,
} from "./podspec";
export {
  readGradlePropertiesVersion,
  writeGradlePropertiesVersion,
  createGradlePropertiesManifestProvider,
} from "./gradle";
export {
  readCargoVersion,
  writeCargoVersion,
  createCargoManifestProvider,
} from "./cargo";
export {
  readPackageJsonVersion,
  writePackageJsonVersion,
  setPackageJsonDependency,
  createPackageJsonManifestProvider,
} from "./package-json";
