package org.xmtp.android.example

import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class MainActivityInstrumentedTest {
    @Test
    fun messengerActivityLaunchesWithTheProcessSession() {
        val appContext = InstrumentationRegistry.getInstrumentation().targetContext
        assertEquals("org.xmtp.android.example", appContext.packageName)
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                assertFalse(activity.isFinishing)
                assertTrue((activity.application as ExampleApp).session.active.value == null)
                assertTrue(activity.findViewById<android.view.ViewGroup>(android.R.id.content).childCount > 0)
            }
        }
    }
}
