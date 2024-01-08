# The Flutter tooling requires that developers have CMake 3.10 or later
# installed. You should not increase this version, as doing so will cause
# the plugin to fail to compile for some customers of the plugin.
cmake_minimum_required(VERSION 3.10)

# Download the binaries if they are not already present.
project("${LibraryName}")
set(ReleaseTag "${LibraryName}-v${ReleaseVersion}")

# If the archive file (xmtp_bindings_flutter.jniLibs.tar.gz) doesn't exist
# then we download it for this release from GitHub.
# Then we untar it into the src/main/jniLibs directory.
set(ArchiveFileName "${LibraryName}.jniLibs.tar.gz")
set(ArchiveUrl "https://github.com/xmtp/libxmtp/releases/download/${ReleaseTag}/${ArchiveFileName}")
set(ArchivePath "${CMAKE_CURRENT_SOURCE_DIR}/${ArchiveFileName}")
set(LibRoot "${CMAKE_CURRENT_SOURCE_DIR}/src/main/jniLibs")

# NOTE: For now, we short-circuit the download by copying from bundled artifacts/
# TODO: actually download the published release at first-build instead
# See also setup-apple-artifacts.sh (which does the same thing for iOS/macOS)
file(COPY "../artifacts/android/${ArchiveFileName}" DESTINATION "${CMAKE_CURRENT_SOURCE_DIR}")

if(NOT EXISTS ${ArchivePath})
  message(STATUS "Downloading ${ArchiveUrl} to ${ArchivePath}")
  file(DOWNLOAD
    ${ArchiveUrl}
    ${ArchivePath}
    TLS_VERIFY ON
    STATUS DownloadStatus
  )
  list(GET DownloadStatus 0 DownloadStatusCode)
  list(GET DownloadStatus 1 DownloadStatusMessage)
  if(${DownloadStatusCode} EQUAL 0)
    message(STATUS "Download succeeded to ${ArchivePath}")
  else()
    file(REMOVE ${ArchivePath})
    message(FATAL_ERROR "Error downloading ${ArchiveUrl} to ${ArchivePath}: ${DownloadStatusMessage}")
  endif()
else()
  message(STATUS "Using previously downloaded ${ArchivePath}")
endif()

# Extract the binaries, overriding any already present.
file(REMOVE_RECURSE ${LibRoot})
file(MAKE_DIRECTORY ${LibRoot})
execute_process(
  COMMAND ${CMAKE_COMMAND} -E tar xzf ${ArchivePath}
  WORKING_DIRECTORY ${LibRoot}
)
