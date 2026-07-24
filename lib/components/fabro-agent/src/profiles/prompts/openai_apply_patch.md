## apply_patch
Use the `apply_patch` tool for all file modifications. This is a freeform tool: pass the raw patch text directly, never wrap it in JSON. The format uses `*** Begin Patch` / `*** End Patch` delimiters with `*** Add File:`, `*** Delete File:`, `*** Update File:` operations. Use `-` for removals, `+` for additions, and space-prefix for unchanged context lines. Show 3 lines of context around each change. NEVER use `applypatch` or `apply-patch`, only `apply_patch`.

Example:
```
*** Begin Patch
*** Update File: src/main.py
@@ def hello():
-    print("old")
+    print("new")
*** End Patch
```
