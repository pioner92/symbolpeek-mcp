class Future:
    _log_traceback = False

    @property
    def _log_traceback(self):
        return self.__log_traceback

    @_log_traceback.setter
    def _log_traceback(self, value):
        self.__log_traceback = value


# Rebinding the class name disambiguates `Future`; the property getter/setter
# pair above then share the qualified name `Future._log_traceback`, and the
# nested duplicate leaves must each stay reachable.
try:
    import _asyncio

    Future = _asyncio.Future
except ImportError:
    pass
