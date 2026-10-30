package io.github.wenshiqi0.kite.media

import android.text.Editable
import android.text.Selection
import android.text.SpannableStringBuilder
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.InputMethodManager
import android.content.Context

/** IME composition is local; only committed text reaches the remote input buffer. */
class RemoteInputConnection(
    private val view:View,
    private val buffer:RemoteTextBuffer,
    private val key:(Int)->Boolean,
    private val event:(KeyEvent)->Boolean,
):BaseInputConnection(view,true) {
    // An immutable space supplies a deletion context for an unknown remote editor.
    private val content=SpannableStringBuilder(" ")
    init {Selection.setSelection(content,1)}
    override fun getEditable():Editable=content
    private fun updateCursor(){
        (view.context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).updateSelection(
            view,Selection.getSelectionStart(content),Selection.getSelectionEnd(content),
            getComposingSpanStart(content),getComposingSpanEnd(content))
    }
    private fun reset(){
        removeComposingSpans(content);content.replace(0,content.length," ");Selection.setSelection(content,1);updateCursor()
    }
    fun cancelComposition(){reset()}
    override fun setSelection(start:Int,end:Int):Boolean =
        super.setSelection(start.coerceIn(1,content.length),end.coerceIn(1,content.length))
    override fun setComposingRegion(start:Int,end:Int):Boolean =
        super.setComposingRegion(start.coerceIn(1,content.length),end.coerceIn(1,content.length))
    override fun setComposingText(text:CharSequence?,newCursorPosition:Int):Boolean {
        val result=super.setComposingText(text,newCursorPosition);updateCursor();return result
    }
    override fun commitText(text:CharSequence?,newCursorPosition:Int):Boolean {
        super.commitText(text,newCursorPosition)
        val value=text?.toString().orEmpty()
        if(value=="\n"||value=="\r"||value=="\r\n"){buffer.flush();key(13)}
        else if(value=="\t"){buffer.flush();key(9)}
        else buffer.append(value)
        reset();return true
    }
    override fun finishComposingText():Boolean {
        val text=content.toString().drop(1)
        super.finishComposingText()
        if(text.isNotEmpty())buffer.append(text)
        reset();return true
    }
    private fun delete(before:Int,after:Int,codePoints:Boolean):Boolean {
        if(before !in 0..4096||after !in 0..4096)return false
        if(getComposingSpanEnd(content)>getComposingSpanStart(content)&&getComposingSpanStart(content)>=0) {
            val result=if(codePoints)super.deleteSurroundingTextInCodePoints(before,after)else super.deleteSurroundingText(before,after)
            if(content.length<=1)reset()else updateCursor()
            return result
        }
        val left=(before-buffer.removeBefore(before,codePoints)).coerceAtLeast(0)
        if(left>0||after>0) {
            buffer.flush()
            repeat(left){key(8)}
            repeat(after){key(46)}
        }
        reset();return true
    }
    override fun deleteSurroundingText(beforeLength:Int,afterLength:Int)=delete(beforeLength,afterLength,false)
    override fun deleteSurroundingTextInCodePoints(beforeLength:Int,afterLength:Int)=delete(beforeLength,afterLength,true)
    override fun sendKeyEvent(event:KeyEvent):Boolean {
        buffer.flush();return this.event(event)
    }
    override fun performEditorAction(actionCode:Int):Boolean {
        buffer.flush();return key(13)
    }
    override fun closeConnection(){reset();buffer.flush();super.closeConnection()}
}
