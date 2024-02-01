package org.xmtp.android.example.conversation

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.View.VISIBLE
import android.view.ViewGroup
import android.widget.Toast
import androidx.core.widget.addTextChangedListener
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.bottomsheet.BottomSheetDialogFragment
import kotlinx.coroutines.launch
import org.xmtp.android.example.R
import org.xmtp.android.example.databinding.BottomSheetNewConversationBinding
import org.xmtp.android.example.databinding.BottomSheetNewGroupBinding
import java.util.regex.Pattern

class NewGroupBottomSheet : BottomSheetDialogFragment() {

    private val viewModel: NewConversationViewModel by viewModels()
    private var _binding: BottomSheetNewGroupBinding? = null
    private val addresses: MutableList<String> = mutableListOf()
    private val binding get() = _binding!!

    companion object {
        const val TAG = "NewGroupBottomSheet"

        private val ADDRESS_PATTERN = Pattern.compile("^0x[a-fA-F0-9]{40}\$")

        fun newInstance(): NewGroupBottomSheet {
            return NewGroupBottomSheet()
        }
    }

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?
    ): View {
        _binding = BottomSheetNewGroupBinding.inflate(inflater, container, false)
        return binding.root
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.uiState.collect(::ensureUiState)
            }
        }

        binding.addressInput1.addTextChangedListener {
            if (viewModel.uiState.value is NewConversationViewModel.UiState.Loading) return@addTextChangedListener
            val input = binding.addressInput1.text.trim()
            val matcher = ADDRESS_PATTERN.matcher(input)
            if (matcher.matches()) {
                addresses.add(input.toString())
                binding.addressInput2.visibility = VISIBLE
            }
        }

        binding.addressInput2.addTextChangedListener {
            if (viewModel.uiState.value is NewConversationViewModel.UiState.Loading) return@addTextChangedListener
            val input = binding.addressInput2.text.trim()
            val matcher = ADDRESS_PATTERN.matcher(input)
            if (matcher.matches()) {
                addresses.add(input.toString())
                viewModel.createGroup(addresses)
            }
        }
    }

    override fun onDestroyView() {
        super.onDestroyView()
        _binding = null
    }

    private fun ensureUiState(uiState: NewConversationViewModel.UiState) {
        when (uiState) {
            is NewConversationViewModel.UiState.Error -> {
                binding.addressInput1.isEnabled = true
                binding.addressInput2.isEnabled = true
                binding.progress.visibility = View.GONE
                showError(uiState.message)
            }
            NewConversationViewModel.UiState.Loading -> {
                binding.addressInput1.isEnabled = false
                binding.addressInput2.isEnabled = false
                binding.progress.visibility = View.VISIBLE
            }
            is NewConversationViewModel.UiState.Success -> {
                startActivity(
                    ConversationDetailActivity.intent(
                        requireContext(),
                        topic = uiState.conversation.topic,
                        peerAddress = uiState.conversation.peerAddress
                    )
                )
                dismiss()
            }
            NewConversationViewModel.UiState.Unknown -> Unit
        }
    }

    private fun showError(message: String) {
        val error = message.ifBlank { resources.getString(R.string.error) }
        Toast.makeText(requireContext(), error, Toast.LENGTH_SHORT).show()
    }
}
