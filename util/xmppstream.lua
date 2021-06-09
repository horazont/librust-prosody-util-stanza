local rxmppstream = require "librprosody".xmppstream;
local exports = {};
for k, v in pairs(rxmppstream) do
	exports[k] = v;
end

exports.global_context = rxmppstream.new_context();

function exports.new(session, extra, size_limit)
	extra = extra or {};
	extra.ctx = extra.ctx or exports.global_context;
	return rxmppstream.new(session, extra, size_limit);
end

function exports.release_global_temporaries()
	exports.global_context.release_temporaries();
end

return exports;
