---
"buttervoice": patch
---

Fix hold-to-record stopping immediately for modifier shortcuts such as Right Option. Check macOS modifier flags when detecting held keys so missed-release recovery does not stop an active hold.
