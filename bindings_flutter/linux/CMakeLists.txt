cmake_minimum_required(VERSION 3.10)

# TODO: set this externally as part of invoking linux build
set(LibraryName "xmtp_bindings_flutter")
set(ReleaseVersion "0.0.1-development.3")

# Download the binaries if they are not already present.
project("${LibraryName}")
set(ReleaseTag "${LibraryName}-v${ReleaseVersion}")

# If the archive file (xmtp_bindings_flutter.linux.tar.gz) doesn't exist
# then we download it for this release from GitHub.
# Then we untar it into this directory.
set(ArchiveFileName "${LibraryName}.linux.tar.gz")
set(ArchiveUrl "https://github.com/xmtp/libxmtp/releases/download/${ReleaseTag}/${ArchiveFileName}")
set(ArchivePath "${CMAKE_CURRENT_SOURCE_DIR}/${ArchiveFileName}")
set(LibRoot "${CMAKE_CURRENT_SOURCE_DIR}/${ReleaseTag}")

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

set(flutter_library_name_bundled_libraries
  "${LibRoot}/${FLUTTER_TARGET_PLATFORM}/lib${LibraryName}.so"
  PARENT_SCOPE
)