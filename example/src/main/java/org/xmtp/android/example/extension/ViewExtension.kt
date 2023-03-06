package org.xmtp.android.example.extension

import android.view.View
import android.view.ViewGroup.MarginLayoutParams

fun View.margins(
    left: Int = 0,
    top: Int = 0,
    right: Int = 0,
    bottom: Int = 0
) {
    val layoutParams = layoutParams as MarginLayoutParams
    val marginLeft = left.let { if (it > 0) it else layoutParams.leftMargin }
    val marginTop = top.let { if (it > 0) it else layoutParams.topMargin }
    val marginRight = right.let { if (it > 0) it else layoutParams.rightMargin }
    val marginBottom = bottom.let { if (it > 0) it else layoutParams.bottomMargin }
    layoutParams.setMargins(marginLeft, marginTop, marginRight, marginBottom)
}
