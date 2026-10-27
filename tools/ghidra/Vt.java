// For a function address: prints references to it; for data references (vtables),
// dumps the 64 pointers around the reference with their function names.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;

public class Vt extends GhidraScript {
    @Override
    public void run() throws Exception {
        Address t = toAddr(getScriptArgs()[0]);
        for (Reference r : getReferencesTo(t)) {
            println("ref " + r.getFromAddress() + " " + r.getReferenceType());
            if (!r.getReferenceType().isData()) continue;
            Address base = r.getFromAddress().subtract(4 * 40);
            for (int i = 0; i < 80; i++) {
                Address p = base.add(4 * i);
                long v = getInt(p) & 0xffffffffL;
                Function f = getFunctionAt(toAddr(v));
                Symbol s = getSymbolAt(p);
                println(p + " " + Long.toHexString(v) + " " + (f == null ? "" : f.getName()) + (s == null ? "" : "   <" + s.getName() + ">"));
            }
        }
    }
}
