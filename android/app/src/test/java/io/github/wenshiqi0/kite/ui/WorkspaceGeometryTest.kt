package io.github.wenshiqi0.kite.ui

import org.junit.Assert.*
import org.junit.Test

class WorkspaceGeometryTest {
    @Test fun split_layout_reserves_chat_space_with_trackpad_open(){
        for(width in listOf(320f,360f,411f))for(height in listOf(592f,732f,866f)){
            val pad=WorkspaceGeometry.padHeight(height)
            val video=WorkspaceGeometry.splitVideoHeight(width,height,16f/9f,pad,60f,0f)
            val chat=height-48-48-pad-60-video
            assertTrue("chat=$chat at $width x $height",chat>=143.9f)
            assertTrue(video>=64)
        }
    }
    @Test fun keyboard_layout_can_fit_without_a_second_input_panel(){
        val video=WorkspaceGeometry.splitVideoHeight(360f,480f,16f/9f,0f,60f,0f)
        assertTrue(480-96-60-video>=144)
        assertTrue(video<=360f*9/16)
    }
}
