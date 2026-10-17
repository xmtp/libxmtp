package uniffi.xmtp_sdk

import org.junit.Assert.assertFalse
import org.junit.Test

class ListenerGatesTest {
    @Test
    fun pendingAfterClientEndCannotStartCallback() {
        val gates = ListenerGates()
        gates.stopAll()
        val gate = ListenerStartGate()
        gates.pending(gate)
        assertFalse(gate.begin())
    }

    @Test
    fun registrationAfterClientEndCannotStartCallback() {
        val gates = ListenerGates()
        gates.stopAll()
        val gate = ListenerStartGate()
        gates.registered(1uL, gate)
        assertFalse(gate.begin())
    }
}
