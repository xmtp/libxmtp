package org.xmtp.android.example.logs

import android.content.Intent
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ArrayAdapter
import android.widget.ListView
import android.widget.TextView
import android.widget.Toast
import androidx.core.content.FileProvider
import com.google.android.material.bottomsheet.BottomSheetDialogFragment
import org.xmtp.android.example.R
import java.io.File

class LogViewerBottomSheet : BottomSheetDialogFragment() {
    companion object {
        const val TAG = "LogViewerBottomSheet"

        fun newInstance(): LogViewerBottomSheet = LogViewerBottomSheet()
    }

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?,
    ): View? = inflater.inflate(R.layout.bottom_sheet_log_viewer, container, false)

    override fun onViewCreated(
        view: View,
        savedInstanceState: Bundle?,
    ) {
        super.onViewCreated(view, savedInstanceState)

        val logsList = view.findViewById<ListView>(R.id.logs_list)
        val emptyText = view.findViewById<TextView>(R.id.empty_text)

        // Get log files from the app's files directory
        val logDirectory = File(requireContext().filesDir, "xmtp_logs")
        val logFiles = if (logDirectory.exists()) logDirectory.listFiles() else null

        if (logFiles.isNullOrEmpty()) {
            logsList.visibility = View.GONE
            emptyText.visibility = View.VISIBLE
        } else {
            logsList.visibility = View.VISIBLE
            emptyText.visibility = View.GONE

            // Sort files by last modified date (newest first)
            val sortedFiles = logFiles.sortedByDescending { it.lastModified() }

            // Create adapter with file names and sizes
            val fileItems =
                sortedFiles.map {
                    "${it.name} (${formatFileSize(it.length())})"
                }
            val adapter = ArrayAdapter(requireContext(), android.R.layout.simple_list_item_1, fileItems)
            logsList.adapter = adapter

            // Handle click on log file
            logsList.setOnItemClickListener { _, _, position, _ ->
                shareLogFile(sortedFiles[position])
            }
        }
    }

    private fun formatFileSize(size: Long): String =
        when {
            size < 1024 -> "$size B"
            size < 1024 * 1024 -> "${size / 1024} KB"
            else -> "${size / (1024 * 1024)} MB"
        }

    private fun shareLogFile(file: File) {
        try {
            val uri =
                FileProvider.getUriForFile(
                    requireContext(),
                    "${requireContext().packageName}.fileprovider",
                    file,
                )

            val shareIntent =
                Intent().apply {
                    action = Intent.ACTION_SEND
                    putExtra(Intent.EXTRA_STREAM, uri)
                    type = "text/plain"
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }

            startActivity(Intent.createChooser(shareIntent, getString(R.string.share_log)))
        } catch (e: Exception) {
            Toast.makeText(context, "Error sharing log: ${e.message}", Toast.LENGTH_SHORT).show()
        }
    }
}
