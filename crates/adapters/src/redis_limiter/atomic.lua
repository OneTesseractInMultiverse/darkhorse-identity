-- Storage compare/exchange only. Rust computes budgets and expired records.
local key = KEYS[1]
local op, generation, epoch, run_id, replication_id, database_ms = unpack(ARGV, 1, 6)
local capacity = 16384
local max_time = 9007199254740991
local function field(text, name)
    return string.match(text, '\r\n' .. name .. ':([^\r\n]+)\r\n')
end
local function compatible(server, replication, memory)
    return field(server, 'run_id') == run_id and field(server, 'redis_mode') == 'standalone'
        and field(replication, 'master_replid') == replication_id and field(replication, 'role') == 'master'
        and field(memory, 'maxmemory_policy') == 'noeviction'
        and tonumber(field(memory, 'maxmemory') or '0') > 0
        and tonumber(field(memory, 'used_memory') or '0') <= tonumber(field(memory, 'maxmemory') or '0')
end
local function timestamp(parts)
    return tonumber(parts[1]) * 1000 + math.floor(tonumber(parts[2]) / 1000)
end
local function valid_clock(now, previous)
    return now <= max_time and now >= previous and math.abs(now - tonumber(database_ms)) <= 1000
end
local function valid_field(value)
    return #value == 66 and string.match(value, '^b:[a-f0-9]+$') ~= nil
end
local function metadata(values, length)
    local count, clock = tonumber(values[3]), tonumber(values[4])
    if values[1] ~= generation or values[2] ~= epoch or not count or not clock
        or count < 0 or count > capacity or count % 1 ~= 0 or length ~= count + 5
        or clock < 0 or clock > max_time or not tonumber(values[5]) then return nil end
    return {count, clock, values[5]}
end
if not compatible(redis.call('INFO','server'), redis.call('INFO','replication'), redis.call('INFO','memory')) then
    return redis.error_reply('UNTRUSTED_INSTANCE')
end
local now = timestamp(redis.call('TIME'))
if not valid_clock(now, 0) then return redis.error_reply('CLOCK_UNSAFE') end
if op == 'initialize' then
    local existing = redis.call('HMGET', key, '_generation', '_epoch')
    if existing[1] == generation and existing[2] == epoch then
        local meta=metadata(redis.call('HMGET',key,'_generation','_epoch','_count','_clock','_cursor'),redis.call('HLEN',key))
        if not meta or not valid_clock(now,meta[2]) then return redis.error_reply('UNTRUSTED_STATE') end
        return 1
    end
    if existing[2] and tonumber(existing[2]) >= tonumber(epoch) then return redis.error_reply('STALE_GENERATION') end
    redis.call('DEL', key)
    redis.call('HSET', key, '_generation', generation, '_epoch', epoch, '_count', '0', '_clock', now, '_cursor', '0')
    return 1
end
local meta = metadata(redis.call('HMGET', key, '_generation','_epoch','_count','_clock','_cursor'), redis.call('HLEN',key))
if not meta or not valid_clock(now,meta[2]) then return redis.error_reply('UNTRUSTED_STATE') end
if op == 'status' then return meta[1] end
if op == 'snapshot' then
    if #ARGV < 7 or #ARGV > 10 then return redis.error_reply('INVALID_ARGUMENTS') end
    local result = {tostring(now),tostring(meta[2])}
    for i=7,#ARGV do
        if not valid_field(ARGV[i]) then return redis.error_reply('INVALID_FIELD') end
        result[#result+1] = redis.call('HGET',key,ARGV[i]) or ''
    end
    return result
end
if op == 'apply' then
    local valid_until = tonumber(ARGV[7])
    local observed = tonumber(ARGV[8])
    if not observed or now < observed then return redis.error_reply('CLOCK_UNSAFE') end
    if not valid_until or now >= valid_until then return 0 end
    if #ARGV < 11 or #ARGV > 20 or (#ARGV-8)%3 ~= 0 then return redis.error_reply('INVALID_ARGUMENTS') end
    local additions = 0
    local update = {'_clock',tostring(now)}
    for i=9,#ARGV,3 do
        if not valid_field(ARGV[i]) or #ARGV[i+1]>96 or #ARGV[i+2]>96 or ARGV[i+2]=='' then return redis.error_reply('INVALID_FIELD') end
        local current=redis.call('HGET',key,ARGV[i]) or ''
        if current ~= ARGV[i+1] then return 0 end
        if current == '' then additions=additions+1 end
        update[#update+1]=ARGV[i]
        update[#update+1]=ARGV[i+2]
    end
    if meta[1]+additions > capacity then return 2 end
    update[#update+1]='_count'
    update[#update+1]=tostring(meta[1]+additions)
    redis.call('HSET',key,unpack(update))
    return 1
end
if op == 'scan' then
    local scanned=redis.call('HSCAN',key,meta[3],'COUNT',128)
    return {tostring(now),tostring(meta[2]),scanned[1],scanned[2]}
end
if op == 'prune' then
    if #ARGV>1032 or (#ARGV-8)%2 ~= 0 or not tonumber(ARGV[7]) or not tonumber(ARGV[8]) then return redis.error_reply('INVALID_ARGUMENTS') end
    if now < tonumber(ARGV[8]) then return redis.error_reply('CLOCK_UNSAFE') end
    local removed={}
    for i=9,#ARGV,2 do
        if not valid_field(ARGV[i]) or #ARGV[i+1]>96 then return redis.error_reply('INVALID_FIELD') end
        if redis.call('HGET',key,ARGV[i])==ARGV[i+1] then removed[#removed+1]=ARGV[i] end
    end
    -- Any interruption/error between these commands leaves the count inconsistent,
    -- which blocks admission; a partial cleanup never grants a fresh allowance.
    if #removed>0 then redis.call('HDEL',key,unpack(removed)) end
    redis.call('HSET',key,'_count',meta[1]-#removed,'_clock',now,'_cursor',ARGV[7])
    return 1
end
return redis.error_reply('INVALID_OPERATION')
