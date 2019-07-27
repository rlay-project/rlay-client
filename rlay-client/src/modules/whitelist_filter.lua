whitelist = { "0x019280031b20440b233fce5d14d7ee5fbdce3736c42f60b9981b62caf441f3d635d5083b1c59" }


local function contains(table, val)
   for i=1,#table do
      if table[i] == val then 
         return true
      end
   end
   return false
end

-- Keep all entities for whom `true` is returned
function filter(entity)
  return contains(whitelist, entity.cid)
end
