-- 0008: the manual-discount caps the owner adopted (merchant decisions
-- 3.1–3.3, microstep 1.4.5), written where 0004's header said they would land.
--
-- 0004 seeded `discount.manual` for the cashier as {"kind":"role_cap"} with no
-- number, because the merchant had not answered, and granted it to the shift
-- lead with no cap at all. On 29 October 2026 the owner adopted the
-- questionnaire's defaults: cashier 5%, shift lead 15%, manager unlimited. The
-- number joins the kind as the sibling key 0004 promised, `max_percent_ppm`
-- (5% is 50000), and the shift lead's cell becomes role-capped like the
-- cashier's. The manager's cell stays granted with no limit, and the owner's
-- stays `sets_the_limit`: the owner sets these numbers and runs no till.
--
-- Each UPDATE matches the exact row 0004 shipped. A cell a merchant has
-- already changed under `user.admin` is left as they set it: a migration
-- writes the vendor default, never over the merchant's own answer.
--
-- `role_capability` is reference data, not a fact table: no I-4 trigger guards
-- it, and nothing appends to an outbox here. The server does not have the
-- table yet; its mirror records what the table must carry when it does.

UPDATE role_capability
   SET limit_json = '{"kind":"role_cap","max_percent_ppm":50000}'
 WHERE role_id = X'01A05F6A580070008000000000000001'   -- cashier
   AND capability = 'discount.manual'
   AND decision = 'granted'
   AND limit_json = '{"kind":"role_cap"}';

UPDATE role_capability
   SET limit_json = '{"kind":"role_cap","max_percent_ppm":150000}'
 WHERE role_id = X'01A05F6A580070008000000000000002'   -- shift lead
   AND capability = 'discount.manual'
   AND decision = 'granted'
   AND limit_json IS NULL;
