"""Definitions guarded by control flow.

Python has no block scope, so each of these belongs to the enclosing module or
class. A walker that only descends into `def`/`class` bodies drops them, and
they become unreachable by any name while still existing in the file.
"""

import sys

try:
    from _fast import loads
except ImportError:

    def loads(raw):
        return {"raw": raw}


if sys.platform == "win32":

    def home():
        return "C:\\Users"

else:

    def home():
        return "/home"


class Store:
    def get(self, key):
        return key

    if sys.version_info >= (3, 12):

        def fast_path(self):
            return True


for _name in ("a", "b"):

    def loop_body():
        return _name


with open(__file__) as _handle:

    def context_body():
        return _handle.name


def plain():
    return 1
