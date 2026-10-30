package io.github.wenshiqi0.kite.media

/** A fixed 200ms batching window, not a debounce that can hold continuous typing. */
class RemoteTextBuffer(
    private val send:(String)->Boolean,
    private val schedule:((()->Unit))->Unit,
    private val failure:()->Unit,
) {
    private val pending=StringBuilder()
    private var scheduled=false
    private var generation=0
    fun append(value:String) {
        if(value.isEmpty())return
        if(value.toByteArray(Charsets.UTF_8).size>256*1024){failure();return}
        pending.append(value)
        if(pending.length>=2048){flush();return}
        if(!scheduled) {
            scheduled=true
            val token=generation
            schedule { if(generation==token)flush() }
        }
    }
    fun removeBefore(length:Int,codePoints:Boolean=false):Int {
        var removed=0
        while(removed<length&&pending.isNotEmpty()) {
            val count=Character.charCount(Character.codePointBefore(pending,pending.length))
            pending.delete(pending.length-count,pending.length)
            removed+=if(codePoints)1 else count
        }
        return removed
    }
    fun flush():Boolean {
        val text=pending.toString()
        pending.clear();scheduled=false;generation++
        var start=0
        while(start<text.length) {
            var end=start;var bytes=0
            while(end<text.length) {
                val cp=Character.codePointAt(text,end)
                val size=when {cp<=0x7f->1;cp<=0x7ff->2;cp<=0xffff->3;else->4}
                if(bytes+size>4096)break
                bytes+=size;end+=Character.charCount(cp)
            }
            if(!send(text.substring(start,end))){failure();return false}
            start=end
        }
        return true
    }
    fun cancel(){pending.clear();scheduled=false;generation++}
}
